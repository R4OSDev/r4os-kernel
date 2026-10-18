//! The early boot cuts are Rust owned; later CPU/clock/input/display owners
//! publish only fixed snapshots or perform their explicitly named work.
use crate::{arch::tables, build_config::config};
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

pub fn puts(text: &[u8]) {
    super::log_route::puts(text);
}
pub fn dec(value: u64) {
    super::log_route::dec(value);
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
    unsafe { cpu_foundation() };
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
    let result = unsafe { boot_console(&mut info) };
    if result != 0 {
        let text: &[u8] = if result == -1 {
            b"No framebuffer in BootInfo"
        } else {
            b"No active DisplayBackend"
        };
        unsafe { super::fatal::r4os_rust_fatal_record(1, text.as_ptr(), text.len()) };
        return false;
    }
    unsafe { boot_foundation_summary() };
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
    super::log_route::putc(b'x');
    dec(info.glyph_height as u64);
    puts(b" font, ");
    dec(info.scale as u64);
    puts(b"x scale)\r\n");
    puts(b"  Kernel base: 0x");
    super::log_route::hex(0xffffffff80000000, 16);
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
    super::log_route::set_output_hook(Some(boot_hook), false);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_boot_release() {
    REDIRECT.store(false, Ordering::Relaxed);
    crate::display::boot_display::invalidate_external();
    crate::display::diag_screen::r4os_rust_diag_reset();
    super::log_route::set_output_hook(Some(boot_hook), false);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_boot_unroute() {
    REDIRECT.store(false, Ordering::Relaxed);
    super::log_route::set_output_hook(None, false);
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
    if config::ENABLE_COM1_DEBUG && super::serial::init_debug() {
        unsafe {
            super::log_route::set_serial(
                Some(serial_putc),
                Some(super::serial::r4os_rust_serial_write),
                Some(super::serial::r4os_rust_serial_flush),
                Some(super::serial::r4os_rust_serial_sync),
            );
        };
    } else if !config::ENABLE_COM1_DEBUG {
        unsafe { super::log_route::set_serial(None, None, None, None) };
    }
}

unsafe fn cpu_foundation() {
    crate::arch::pic::init();
    crate::platform::cpu::detect();
    crate::platform::module_cpu::require_hardware();
    crate::arch::smp::r4os_rust_smp_control(0, 0);
    if let Err(reason) = crate::arch::pat::adopt_bsp() {
        crate::boot::log_route::puts(b"[PAT] BSP contract rejected reason=");
        crate::boot::log_route::puts(reason);
        crate::boot::log_route::puts(b"\r\n");
        crate::boot::fatal::kernel(2, reason);
    }
    crate::platform::clocksource::configure_cpu();
    if !crate::arch::fpu::init() {
        crate::boot::fatal::kernel(2, b"FPU/SSE task-state init failed");
    }
    crate::platform::module_cpu::admit_bsp();
}

unsafe fn boot_console(output: *mut IntroInfo) -> i32 {
    let Some(state) = crate::display::boot_display::get() else {
        return -1;
    };
    let stats = crate::display::manager::stats();
    if !stats.registered {
        return -2;
    }
    let mode = stats.mode;
    let (w, h) = crate::font::owner::with_catalog(|s| (s.glyph_width(), s.glyph_height()));
    unsafe {
        output.write(IntroInfo {
            width: mode.width,
            height: mode.height,
            bpp: u32::from(mode.bpp),
            cols: state.console_metrics.cols,
            rows: state.console_metrics.rows,
            scale: state.console_metrics.scale,
            glyph_width: w,
            glyph_height: h,
        });
    }
    0
}

unsafe fn boot_foundation_summary() {
    use crate::{boot::log_route as l, input::boot as input, platform::timer};
    l::puts(b"  Booted via Limine ");
    l::puts(b"[OK]\r\n");
    l::puts(b"  CPU tables / PIC ");
    l::puts(b"[OK]\r\n");
    l::puts(b"  Timer core ");
    l::puts(timer::backend_name());
    l::puts(b" ");
    l::dec(u64::from(timer::frequency()));
    l::puts(b" Hz [OK]\r\n");
    l::puts(b"  Keyboard IRQ ");
    l::puts(if input::r4os_rust_input_boot_keyboard_ready() {
        b"[OK]\r\n"
    } else {
        b"[not ready]\r\n"
    });
    l::puts(b"  Mouse IRQ ");
    l::puts(match input::r4os_rust_input_boot_mouse_status() {
        0 => b"[not ready]\r\n",
        1 => b"[OK]\r\n",
        2 => b"[not found]\r\n",
        3 => b"[no display]\r\n",
        _ => unreachable!(),
    });
}
