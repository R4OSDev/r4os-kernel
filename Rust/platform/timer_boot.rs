//! One-time BSP PIT foundation, before AP/task startup. Completion belongs to
//! this boot phase, independently of later timer backend initialization.
use crate::{
    arch::pit,
    platform::{monotonic, timer},
};
use core::sync::atomic::{AtomicBool, Ordering};

static INITIALIZED: AtomicBool = AtomicBool::new(false);

#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_timer_boot_control(operation: u8) -> bool {
    match operation {
        0 => {
            if !INITIALIZED.load(Ordering::Acquire) {
                timer::init_pit(pit::DEFAULT_HZ);
                monotonic::attach_periodic();
                INITIALIZED.store(true, Ordering::Release);
            }
            true
        }
        1 => INITIALIZED.load(Ordering::Acquire),
        _ => false,
    }
}
