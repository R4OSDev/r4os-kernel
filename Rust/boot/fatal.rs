//! Early failure state and terminal decisions do not depend on heap/userland.
use super::crash::{self, BuildInput, FixedText};
use core::ptr::{addr_of, addr_of_mut, null};
use core::sync::atomic::{AtomicU8, Ordering};
#[derive(Clone, Copy, Default)]
struct Pending {
    active: bool,
    phase: u8,
    message: FixedText,
}
static PHASE: AtomicU8 = AtomicU8::new(1);
// Pending failures are published/consumed only by the linear BSP boot owner.
// Runtime panics use their own stack record and the atomic phase/depth instead.
static mut PENDING: Pending = Pending {
    active: false,
    phase: 0,
    message: FixedText {
        len: 0,
        bytes: [0; 128],
    },
};

pub fn halt() -> ! {
    loop {
        unsafe { core::arch::asm!("cli", "hlt", options(nomem, nostack)) };
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_fatal_init() {
    crate::platform::clocksource::early_init();
    super::performance::init();
    PHASE.store(1, Ordering::Relaxed);
    unsafe { addr_of_mut!(PENDING).write(Pending::default()) };
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_fatal_phase(phase: u8) {
    PHASE.store(phase, Ordering::Relaxed);
    super::performance::record(phase);
}
#[unsafe(no_mangle)]
/// # Safety
/// Called by the BSP boot owner with resident borrowed message bytes.
pub unsafe extern "C" fn r4os_rust_fatal_record(phase: u8, message: *const u8, len: usize) {
    PHASE.store(phase, Ordering::Relaxed);
    let mut pending = Pending {
        active: true,
        phase,
        ..Pending::default()
    };
    if len != 0 {
        pending
            .message
            .set(unsafe { core::slice::from_raw_parts(message, len.min(128)) });
    }
    unsafe {
        addr_of_mut!(PENDING).write(pending);
        fatal_record_display(message, len);
    }
}
pub fn kernel(phase: u8, message: &[u8]) -> ! {
    if crate::arch::machine_check::terminal_active() {
        halt();
    }
    PHASE.store(phase, Ordering::Relaxed);
    terminal(5, phase, message)
}
pub fn rust_panic(message: &[u8]) -> ! {
    kernel(PHASE.load(Ordering::Relaxed), message)
}
fn terminal(cause: u8, phase: u8, message: &[u8]) -> ! {
    super::performance::fail_fatal();
    unsafe { fatal_display(message.as_ptr(), message.len()) };
    let input = BuildInput {
        cause,
        phase,
        ticks: unsafe { fatal_ticks() },
        cpu: null(),
        cr2: 0,
        memory: null(),
        context: null(),
        message: message.as_ptr(),
        message_len: message.len(),
    };
    let report = unsafe { crash::build(&input) };
    let reentrant = crash::r4os_rust_crash_enter() != 0;
    unsafe { crash_output(&report, reentrant) };
    halt()
}
#[unsafe(no_mangle)]
/// # Safety
/// The BSP boot owner retains resident fallback bytes until terminal halt.
pub unsafe extern "C" fn r4os_rust_fatal_pending(phase: u8, message: *const u8, len: usize) -> ! {
    let pending = unsafe { addr_of!(PENDING).read() };
    if pending.active {
        kernel(pending.phase, pending.message.bytes());
    }
    let text = if len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(message, len) }
    };
    kernel(phase, text)
}
#[unsafe(no_mangle)]
/// # Safety
/// message retains len resident readable bytes until terminal halt.
pub unsafe extern "C" fn r4os_rust_kernel_fatal(phase: u8, message: *const u8, len: usize) -> ! {
    let text = if len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(message, len) }
    };
    kernel(phase, text)
}

#[unsafe(no_mangle)]
/// # Safety
/// message retains len resident readable bytes until terminal halt.
pub unsafe extern "C" fn r4os_rust_panic_fatal(message: *const u8, len: usize) -> ! {
    let text = if len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(message, len) }
    };
    rust_panic(text)
}

unsafe fn fatal_ticks() -> u64 {
    crate::platform::timer::r4os_rust_timer_control(5, 0, 0)
}

unsafe fn fatal_display(message: *const u8, len: usize) {
    unsafe { crate::display::bootscreen::set_error(message, len) }
}

unsafe fn fatal_record_display(message: *const u8, len: usize) {
    unsafe { crate::display::bootscreen::set_error(message, len) };
    crate::boot::foundation::r4os_rust_boot_unroute();
}

unsafe fn crash_output(report: *const crash::CrashReport, reentrant: bool) {
    crate::boot::foundation::r4os_rust_boot_unroute();
    if reentrant {
        unsafe { crate::display::crash_screen::r4os_rust_crash_screen_serial(report) }
    } else {
        let _ = unsafe { crate::display::crash_screen::r4os_rust_crash_screen_render(report) };
    }
}
