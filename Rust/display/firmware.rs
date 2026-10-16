//! Typed lease projection to the existing firmware/display owner. The token is
//! the exact original task unwind anchor; no second admission or gate is kept.
use crate::sched::task_context::Unwind;
unsafe extern "C" {
    fn r4os_zig_screen_firmware_acquire(out: *mut Unwind) -> bool;
    fn r4os_zig_screen_firmware_release(token: *const Unwind);
}
pub struct Lease(Unwind);
impl Lease {
    pub fn acquire() -> Option<Self> {
        let mut token = Unwind::EMPTY;
        if unsafe { r4os_zig_screen_firmware_acquire(&mut token) } {
            Some(Self(token))
        } else {
            None
        }
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        unsafe { r4os_zig_screen_firmware_release(&self.0) }
    }
}
