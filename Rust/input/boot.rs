//! Original linear BSP PS/2 bootstrap. Registry/display adapters hold their
//! existing typed contracts; this module alone publishes bootstrap progress.
use super::{
    keyboard,
    mouse::{self, Bounds},
};
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
static SLOT: AtomicUsize = AtomicUsize::new(usize::MAX);
static KEYBOARD_INITIALIZED: AtomicBool = AtomicBool::new(false);
static COMPLETED: AtomicBool = AtomicBool::new(false);
static MOUSE_STATUS: AtomicU8 = AtomicU8::new(0);

#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_boot_keyboard() {
    if KEYBOARD_INITIALIZED.load(Ordering::Acquire) {
        return;
    }
    if SLOT.load(Ordering::Acquire) == usize::MAX {
        SLOT.store(unsafe { input_boot_registry_begin() }, Ordering::Release);
    }
    keyboard::init();
    KEYBOARD_INITIALIZED.store(true, Ordering::Release);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_boot_keyboard_ready() -> bool {
    KEYBOARD_INITIALIZED.load(Ordering::Acquire)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_boot_mouse() {
    if MOUSE_STATUS.load(Ordering::Acquire) != 0 {
        return;
    }
    let mut bounds = Bounds::default();
    if !unsafe { input_boot_mode(&mut bounds) } {
        MOUSE_STATUS.store(3, Ordering::Release);
        return;
    }
    MOUSE_STATUS.store(if mouse::init(bounds) { 1 } else { 2 }, Ordering::Release);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_boot_mouse_status() -> u8 {
    MOUSE_STATUS.load(Ordering::Acquire)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_boot_complete() {
    if COMPLETED.load(Ordering::Acquire) {
        return;
    }
    let slot = SLOT.load(Ordering::Acquire);
    if slot != usize::MAX {
        if KEYBOARD_INITIALIZED.load(Ordering::Acquire) {
            {
                crate::driver::registry::set_state_logged(slot, 2);
                crate::driver::registry::set_state_logged(slot, 3);
            }
        } else {
            {
                crate::driver::registry::set_state_logged(slot, 6);
            }
        }
    }
    COMPLETED.store(true, Ordering::Release);
}

unsafe fn input_boot_mode(out: *mut Bounds) -> bool {
    let s = crate::display::manager::stats();
    if !s.registered {
        return false;
    }
    unsafe {
        out.write(Bounds {
            width: s.mode.width,
            height: s.mode.height,
        });
    }
    true
}

unsafe fn input_boot_registry_begin() -> usize {
    unsafe { crate::driver::registry::begin_logged(b"PS2", 3, 1, 0).unwrap_or(usize::MAX) }
}
