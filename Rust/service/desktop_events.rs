//! IRQ-safe desktop activity revision and lost-wakeup-safe enrollment under
//! the same scheduler runtime owner as the existing WaitQueue transport.
use crate::sched::{
    sync_types::{Text, WaitQueue, WaitRequest},
    sync_wait,
};
use core::{cell::UnsafeCell, ffi::c_void, ptr};
struct Activity {
    queue: UnsafeCell<WaitQueue>,
    sequence: UnsafeCell<u64>,
}
// SAFETY: every sequence read/write and queue enrollment is admitted by the
// existing scheduler runtime; predicate runs under that same owner.
unsafe impl Sync for Activity {}
static ACTIVITY: Activity = Activity {
    queue: UnsafeCell::new(WaitQueue::EMPTY),
    sequence: UnsafeCell::new(0),
};
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_desktop_signal() {
    unsafe {
        sync_wait::r4os_rust_sync_queue(
            ACTIVITY.queue.get(),
            8,
            ptr::null_mut(),
            0,
            ACTIVITY.sequence.get(),
        )
    };
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_desktop_sequence() -> u64 {
    unsafe {
        sync_wait::r4os_rust_sync_queue(
            ACTIVITY.queue.get(),
            9,
            ptr::null_mut(),
            0,
            ACTIVITY.sequence.get(),
        )
    }
}
unsafe extern "C" fn unchanged(raw: *mut c_void) -> bool {
    unsafe { *ACTIVITY.sequence.get() == *raw.cast::<u64>() }
}
/// # Safety
/// `output` is writable for this call; no output or sequence borrow crosses
/// the original sleep/enrollment boundary.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_desktop_wait(last: u64, timeout: u64, output: *mut u64) -> i32 {
    let mut observed = last;
    let request = WaitRequest::predicate(
        timeout,
        Text::literal(b"desk-activity"),
        unchanged,
        ptr::from_mut(&mut observed).cast(),
    );
    let result = unsafe { sync_wait::r4os_rust_sync_wait(ACTIVITY.queue.get(), &request) };
    match result {
        1 | 2 => {
            let current = r4os_rust_desktop_sequence();
            unsafe {
                output.write(current);
            }
            i32::from(current != last || result == 1)
        }
        3 | 4 => -2,
        _ => -3,
    }
}
