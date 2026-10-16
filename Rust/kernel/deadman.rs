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

fn write(text: &[u8]) {
    unsafe { kernel_write(text.as_ptr(), text.len()) };
}
fn screen(operation: u8, value: u64, text: &[u8]) -> u64 {
    unsafe { deadman_screen(operation, value, text.as_ptr(), text.len()) }
}
fn storage(operation: u8, now: u64) -> bool {
    unsafe { deadman_storage(operation, now, DEADLOCK) }
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
            deadman_snapshot(true, &snapshot);
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
            deadman_snapshot(false, &snapshot);
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

unsafe fn kernel_write(text: *const u8, len: usize) {
    unsafe { crate::boot::log_route::r4os_rust_log_write(0, text, len) }
}

unsafe fn deadman_screen(operation: u8, value: u64, text: *const u8, len: usize) -> u64 {
    use crate::display::diag_screen as d;
    match operation {
        0 => d::begin_resolvable(),
        1 => u64::from(d::resolve(value)),
        2 => {
            unsafe { d::write(text, len) };
            0
        }
        3 => {
            d::end_line();
            0
        }
        4 => {
            unsafe { d::write(text, len) };
            d::end_line();
            0
        }
        _ => unreachable!(),
    }
}

unsafe fn deadman_storage(operation: u8, now: u64, threshold: u64) -> bool {
    unsafe {
        match operation {
            0 => crate::storage::block_diagnostics::r4os_rust_block_stalled(now, threshold),
            1 | 2 => {
                crate::storage::block_diagnostics::r4os_rust_block_dump(
                    now,
                    threshold,
                    operation == 1,
                );
                true
            }
            _ => panic!("Invalid watchdog storage query"),
        }
    }
}

unsafe fn deadman_snapshot(diag: bool, snapshot: *const snapshots::Deadman) {
    let s = unsafe { snapshot.read() };
    if diag {
        use crate::display::diag_screen::{end_line, number, text as puts};
        puts(b"[DEADMAN] blocked snapshot (! = mutex holder)");
        end_line();
        for e in &s.tasks[..s.count] {
            puts(if e.suspicious_mutex_holder {
                b"!#"
            } else {
                b" #"
            });
            number(u64::from(e.id), false);
            puts(b" ");
            puts(&e.name[..usize::from(e.name_len)]);
            puts(b" w=");
            puts(&e.reason[..usize::from(e.reason_len)]);
            puts(b" lk=");
            number(u64::from(e.held_lock_count), false);
            puts(b" ug=");
            number(u64::from(e.unwind_guard_count), false);
            puts(b" t=");
            number(e.blocked_for_ticks, false);
            end_line();
        }
        if s.count == 0 {
            puts(b"  (no blocked tasks captured)");
            end_line();
        }
        if s.omitted != 0 {
            puts(b"  ... blocked tasks omitted=");
            number(u64::from(s.omitted), false);
            end_line();
        }
    } else {
        use crate::boot::log_route::{dec, hex, puts};
        puts(b"[DEADMAN] blocked snapshot total=");
        dec(u64::from(s.blocked_total));
        puts(b" captured=");
        dec(s.count as u64);
        puts(b" omitted=");
        dec(u64::from(s.omitted));
        puts(b"\r\n");
        for e in &s.tasks[..s.count] {
            puts(if e.suspicious_mutex_holder {
                b"  !#"
            } else {
                b"   #"
            });
            dec(u64::from(e.id));
            puts(b" ");
            puts(&e.name[..usize::from(e.name_len)]);
            puts(b" wait=");
            puts(&e.reason[..usize::from(e.reason_len)]);
            puts(b" object=0x");
            hex(e.wait_object, 16);
            puts(b" lock=");
            dec(u64::from(e.held_lock_count));
            puts(b" unwind=");
            dec(u64::from(e.unwind_guard_count));
            puts(b" blocked_for=");
            dec(e.blocked_for_ticks);
            puts(b"\r\n");
        }
    }
}
