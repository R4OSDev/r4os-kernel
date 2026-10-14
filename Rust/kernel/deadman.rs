//! Original cooperative watchdog; no storage I/O and no ability to preempt a
//! hard spin. Task/incident identities stay retained through the bounded dump.
use crate::{
    platform::timer,
    sched::{
        scheduler,
        task::{
            task_resources as resources, task_snapshots as snapshots,
            task_types::{Bytes, CreateFailure, Role, State},
        },
    },
};
use core::{
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};
static STARTED: AtomicBool = AtomicBool::new(false);
const SCAN: u64 = 5 * crate::arch::pit::DEFAULT_HZ as u64;
const DEADLOCK: u64 = 30 * crate::arch::pit::DEFAULT_HZ as u64;
const REDUMP: u64 = 120 * crate::arch::pit::DEFAULT_HZ as u64;
unsafe extern "C" {
    // Stateless owner cuts, removed with their later storage/display migrations.
    fn r4os_zig_deadman_storage(operation: u8, now: u64, threshold: u64) -> bool;
    fn r4os_zig_deadman_screen(operation: u8, value: u64, text: *const u8, len: usize) -> u64;
    fn r4os_zig_deadman_snapshot(diag: bool, snapshot: *const snapshots::Deadman);
    fn r4os_zig_kernel_write(text: *const u8, len: usize);
}
fn write(text: &[u8]) {
    unsafe { r4os_zig_kernel_write(text.as_ptr(), text.len()) };
}
fn screen(operation: u8, value: u64, text: &[u8]) -> u64 {
    unsafe { r4os_zig_deadman_screen(operation, value, text.as_ptr(), text.len()) }
}
fn storage(operation: u8, now: u64) -> bool {
    unsafe { r4os_zig_deadman_storage(operation, now, DEADLOCK) }
}
unsafe extern "C" fn main() {
    let mut last_dump = 0;
    loop {
        unsafe {
            scheduler::r4os_rust_scheduler_task(
                4,
                ptr::null_mut(),
                SCAN,
                0,
                b"deadman-idle".as_ptr(),
                12,
            );
        }
        let now = timer::tick_count();
        let snapshot = unsafe { snapshots::deadman(now, DEADLOCK) };
        let wedged = snapshot.wedged_mutex_holder;
        let stalled = storage(0, now);
        if !wedged && !stalled {
            last_dump = 0;
            continue;
        }
        if last_dump != 0 && now.wrapping_sub(last_dump) < REDUMP {
            continue;
        }
        last_dump = now;
        let incident = screen(0, 0, b"");
        screen(2, 0, b"[DEADMAN] wedged mutex_holder=");
        screen(2, 0, if wedged { b"yes" } else { b"no" });
        screen(2, 0, b" stalled_storage_exec=");
        screen(2, 0, if stalled { b"yes" } else { b"no" });
        screen(3, 0, b"");
        if stalled {
            storage(1, now);
        }
        unsafe {
            r4os_zig_deadman_snapshot(true, &snapshot);
        }
        screen(4, 0, b"[DEADMAN] end of dump");
        screen(1, incident, b"");
        write(b"\r\n[DEADMAN] wedged: mutex_holder=");
        write(if wedged { b"yes" } else { b"no" });
        write(b" stalled_storage_exec=");
        write(if stalled { b"yes" } else { b"no" });
        write(b"\r\n");
        if stalled {
            storage(2, now);
        }
        unsafe {
            r4os_zig_deadman_snapshot(false, &snapshot);
        }
        write(b"[DEADMAN] end of dump\r\n");
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_deadman_start() -> bool {
    if STARTED.load(Ordering::Acquire) {
        return true;
    }
    let mut failure = CreateFailure::None;
    let task = unsafe {
        resources::create_critical(
            Bytes {
                pointer: b"deadman".as_ptr(),
                len: 7,
            },
            State::Ready,
            main,
            Role::Batch,
            &mut failure,
        )
    };
    if !task.is_null() {
        STARTED.store(true, Ordering::Release);
    }
    STARTED.load(Ordering::Acquire)
}
