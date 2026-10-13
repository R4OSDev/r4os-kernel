//! The early boot cuts are Rust owned; later CPU/clock/input/display owners
//! publish only fixed snapshots or perform their explicitly named work.
use crate::{arch::tables, build_bridge::config};
use core::sync::atomic::{AtomicBool, Ordering};
static CPU_INITIALIZED: AtomicBool = AtomicBool::new(false);
static REDIRECT: AtomicBool = AtomicBool::new(false);
#[repr(C)]
#[derive(Default)]
pub struct IntroInfo {
    width: u32,
    height: u32,
    bpp: u32,
    cols: u32,
    rows: u32,
    scale: u32,
    glyph_width: u32,
    glyph_height: u32,
}
type Putc = unsafe extern "C" fn(u8);
type Write = unsafe extern "C" fn(*const u8, usize);
type Control = extern "C" fn();
unsafe extern "C" {
    fn r4os_zig_cpu_foundation();
    fn r4os_zig_kernel_write(text: *const u8, len: usize);
    fn r4os_zig_kernel_dec(value: u64);
    fn r4os_zig_kernel_hex(value: u64, width: u8);
    fn r4os_zig_kernel_putc(byte: u8);
    fn r4os_zig_boot_console(output: *mut IntroInfo) -> i32;
    fn r4os_zig_boot_foundation_summary();
    fn r4os_zig_boot_route(hook: Option<extern "C" fn(u8) -> bool>);
    fn r4os_zig_boot_release_display();
    fn r4os_zig_kernel_serial_sinks(
        putc: Option<Putc>,
        write: Option<Write>,
        flush: Option<Control>,
        sync: Option<Control>,
    );
}
pub fn puts(text: &[u8]) {
    unsafe { r4os_zig_kernel_write(text.as_ptr(), text.len()) };
}
pub fn dec(value: u64) {
    unsafe { r4os_zig_kernel_dec(value) };
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_cpu_boot() {
    if CPU_INITIALIZED.load(Ordering::Relaxed) {
        return;
    }
    tables::r4os_rust_gdt_init(0);
    puts(b"  GDT loaded ");
    puts(b"[OK]\r\n");
    puts(b"  TSS loaded ");
    puts(b"[OK]\r\n");
    tables::r4os_rust_idt_init();
    puts(b"  IDT loaded ");
    puts(b"[OK]\r\n");
    unsafe { r4os_zig_cpu_foundation() };
    CPU_INITIALIZED.store(true, Ordering::Relaxed);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_cpu_ready() -> bool {
    CPU_INITIALIZED.load(Ordering::Relaxed)
}

#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_boot_intro() -> bool {
    puts(b"\r\n=== R4OS x86_64 (Limine boot) ===\r\n");
    if unsafe { (*super::info::r4os_rust_boot_info()).flags } & 1 == 0 {
        let text = b"BootInfo init failed";
        unsafe { super::fatal::r4os_rust_fatal_record(1, text.as_ptr(), text.len()) };
        return false;
    }
    let mut info = IntroInfo::default();
    let result = unsafe { r4os_zig_boot_console(&mut info) };
    if result != 0 {
        let text: &[u8] = if result == -1 {
            b"No framebuffer in BootInfo"
        } else {
            b"No active DisplayBackend"
        };
        unsafe { super::fatal::r4os_rust_fatal_record(1, text.as_ptr(), text.len()) };
        return false;
    }
    unsafe { r4os_zig_boot_foundation_summary() };
    if config::ENABLE_EXCEPTION_TEST {
        puts(b"  Triggering test exception...\r\n");
        unsafe { core::arch::asm!("ud2", options(nostack)) };
        super::fatal::halt();
    }
    if config::ENABLE_GENERAL_PROTECTION_TEST {
        puts(b"  Triggering test general protection fault...\r\n");
        unsafe { core::arch::asm!("mov ax, 0xffff", "mov ds, ax", out("ax") _, options(nostack)) };
        super::fatal::halt();
    }
    puts(b"  Long mode active\r\n");
    puts(b"  Framebuffer: ");
    dec(info.width as u64);
    puts(b" x ");
    dec(info.height as u64);
    puts(b" @ ");
    dec(info.bpp as u64);
    puts(b" bpp\r\n");
    puts(b"  Console: ");
    dec(info.cols as u64);
    puts(b" cols x ");
    dec(info.rows as u64);
    puts(b" rows (");
    dec(info.glyph_width as u64);
    unsafe { r4os_zig_kernel_putc(b'x') };
    dec(info.glyph_height as u64);
    puts(b" font, ");
    dec(info.scale as u64);
    puts(b"x scale)\r\n");
    puts(b"  Kernel base: 0x");
    unsafe { r4os_zig_kernel_hex(0xffffffff80000000, 16) };
    puts(b"\r\n\r\n");
    true
}
extern "C" fn boot_hook(byte: u8) -> bool {
    unsafe { super::log::r4os_rust_bootlog_write(&byte, 1) };
    true
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_boot_redirect() {
    REDIRECT.store(true, Ordering::Relaxed);
    unsafe { r4os_zig_boot_route(Some(boot_hook)) };
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_boot_release() {
    REDIRECT.store(false, Ordering::Relaxed);
    unsafe {
        r4os_zig_boot_release_display();
        r4os_zig_boot_route(Some(boot_hook));
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_boot_unroute() {
    REDIRECT.store(false, Ordering::Relaxed);
    unsafe { r4os_zig_boot_route(None) };
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_boot_redirect_active() -> bool {
    REDIRECT.load(Ordering::Relaxed)
}
unsafe extern "C" fn serial_putc(byte: u8) {
    unsafe { super::serial::r4os_rust_serial_write(&byte, 1) };
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_com_debug_boot() {
    if config::ENABLE_COM1_DEBUG {
        super::serial::r4os_rust_serial_init(0x3f8);
        unsafe {
            r4os_zig_kernel_serial_sinks(
                Some(serial_putc),
                Some(super::serial::r4os_rust_serial_write),
                Some(super::serial::r4os_rust_serial_flush),
                Some(super::serial::r4os_rust_serial_sync),
            )
        };
    } else {
        unsafe { r4os_zig_kernel_serial_sinks(None, None, None, None) };
    }
}
