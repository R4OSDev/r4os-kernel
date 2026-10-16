//! Original bounded SYNCCHECK cases: recursive contention, auto-reset event
//! ping-pong and the exact caller IF on/off through both blocking paths.
use super::{
    sync_locks, sync_objects,
    sync_types::{Event, Mutex},
    task::{
        task_resources as resources,
        task_types::{Bytes, CreateFailure, ExecutionOwnerKind, Role, State},
    },
};
use crate::{arch::runtime, platform::timer, sched::scheduler};
use core::{
    cell::UnsafeCell,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};
const EVENT_ITERS: u32 = 200;
struct Objects(UnsafeCell<(Mutex, Event, Event)>);
// SAFETY: original selftest runs on the boot caller and BSP-only partner Tasks;
// actual object mutations use native synchronization's runtime admissions.
unsafe impl Sync for Objects {}
static OBJECTS: Objects = Objects(UnsafeCell::new((Mutex::EMPTY, Event::AUTO, Event::AUTO)));
static STOP: AtomicBool = AtomicBool::new(false);
static HOLDER_DONE: AtomicBool = AtomicBool::new(false);
static PARTNER_DONE: AtomicBool = AtomicBool::new(false);
static IF_PARTNER_DONE: AtomicBool = AtomicBool::new(false);
fn mutex() -> *mut Mutex {
    unsafe { ptr::addr_of_mut!((*OBJECTS.0.get()).0) }
}
fn event() -> *mut Event {
    unsafe { ptr::addr_of_mut!((*OBJECTS.0.get()).1) }
}
fn if_event() -> *mut Event {
    unsafe { ptr::addr_of_mut!((*OBJECTS.0.get()).2) }
}
fn sleep(ticks: u64, reason: &[u8]) {
    unsafe {
        scheduler::r4os_rust_scheduler_task(
            4,
            ptr::null_mut(),
            ticks,
            0,
            reason.as_ptr(),
            reason.len(),
        );
    }
}
fn exit() -> ! {
    scheduler::r4os_rust_scheduler_control(12, 0, 0);
    panic!("Task exit returned")
}

fn puts(text: &[u8]) {
    unsafe { kernel_write(text.as_ptr(), text.len()) };
}
fn fail(reason: &[u8]) -> bool {
    puts(b"SYNCCHECK FAIL reason=");
    puts(reason);
    puts(b"\r\n");
    false
}
fn spawn(name: &'static [u8], entry: unsafe extern "C" fn()) -> bool {
    unsafe {
        let mut failure = CreateFailure::None;
        !resources::create(
            resources::CreateArgs {
                name: Bytes {
                    pointer: name.as_ptr(),
                    len: name.len(),
                },
                state: State::Ready,
                needs_fpu: true,
                entry: Some(entry),
                role: Role::Interactive,
                smp_eligible: false,
                smp_r4x: false,
                owner_kind: ExecutionOwnerKind::None,
                owner_context: ptr::null_mut(),
            },
            &mut failure,
        )
        .is_null()
    }
}
unsafe extern "C" fn holder() {
    unsafe {
        sync_locks::r4os_rust_sync_mutex(mutex(), 1, u64::MAX);
        while !STOP.load(Ordering::Acquire) {
            sync_locks::r4os_rust_sync_mutex(mutex(), 2, 0);
            if !sync_locks::r4os_rust_sync_mutex(mutex(), 0, 0) {
                sync_locks::r4os_rust_sync_mutex(mutex(), 1, u64::MAX);
            }
            scheduler::yield_task();
        }
        sync_locks::r4os_rust_sync_mutex(mutex(), 2, 0);
        HOLDER_DONE.store(true, Ordering::Release);
        exit()
    }
}
unsafe extern "C" fn partner() {
    unsafe {
        for _ in 0..EVENT_ITERS {
            sync_objects::r4os_rust_sync_event(event(), 0, 0);
            let mut spin = 0;
            while ptr::addr_of!((*event()).signaled).read_volatile() && spin < 200 {
                spin += 1;
                scheduler::yield_task();
            }
            scheduler::yield_task();
        }
        PARTNER_DONE.store(true, Ordering::Release);
        exit()
    }
}
unsafe extern "C" fn if_partner() {
    sleep(2, b"sync-st-if-signal");
    IF_PARTNER_DONE.store(true, Ordering::Release);
    unsafe { sync_objects::r4os_rust_sync_event(if_event(), 0, 0) };
    exit()
}
fn sleep_preserves_if(enabled: bool) -> bool {
    let original = runtime::r4os_rust_runtime_control(2, 0);
    if enabled {
        runtime::r4os_rust_runtime_control(1, 0);
    }
    sleep(
        1,
        if enabled {
            b"sync-st-if-sleep-on"
        } else {
            b"sync-st-if-sleep-off"
        },
    );
    let after = runtime::r4os_rust_runtime_control(2, 0);
    runtime::r4os_rust_runtime_control(3, original);
    (after & (1 << 9) != 0) == enabled
}
fn event_preserves_if(enabled: bool) -> bool {
    unsafe {
        if_event().write(Event::AUTO);
        IF_PARTNER_DONE.store(false, Ordering::Release);
        if !spawn(b"sync-st-if-evt", if_partner) {
            return false;
        }
        let original = runtime::r4os_rust_runtime_control(2, 0);
        if enabled {
            runtime::r4os_rust_runtime_control(1, 0);
        }
        let result = sync_objects::r4os_rust_sync_event(if_event(), 2, 200);
        let after = runtime::r4os_rust_runtime_control(2, 0);
        runtime::r4os_rust_runtime_control(3, original);
        result == 1 && IF_PARTNER_DONE.load(Ordering::Acquire) && (after & (1 << 9) != 0) == enabled
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_sync_selftest() -> bool {
    unsafe {
        STOP.store(false, Ordering::Release);
        HOLDER_DONE.store(false, Ordering::Release);
        if !spawn(b"sync-st-hold", holder) {
            return fail(b"holder-spawn");
        }
        sleep(3, b"sync-st-warm");
        let t0 = timer::tick_count();
        let got = sync_locks::r4os_rust_sync_mutex(mutex(), 1, 50);
        let elapsed = timer::tick_count().wrapping_sub(t0);
        if got {
            sync_locks::r4os_rust_sync_mutex(mutex(), 2, 0);
        }
        STOP.store(true, Ordering::Release);
        let mut guard = 0;
        while !HOLDER_DONE.load(Ordering::Acquire) && guard < 2000 {
            guard += 1;
            sleep(1, b"sync-st-join");
        }
        if guard >= 2000 {
            return fail(b"holder-join");
        }
        if elapsed > 200 {
            return fail(b"mutex-unbounded");
        }
        event().write(Event::AUTO);
        PARTNER_DONE.store(false, Ordering::Release);
        if !spawn(b"sync-st-evt", partner) {
            return fail(b"partner-spawn");
        }
        for _ in 0..EVENT_ITERS {
            if sync_objects::r4os_rust_sync_event(event(), 2, 200) != 1 {
                return fail(b"event-lost");
            }
        }
        guard = 0;
        while !PARTNER_DONE.load(Ordering::Acquire) && guard < 2000 {
            guard += 1;
            sleep(1, b"sync-st-join2");
        }
        if guard >= 2000 {
            return fail(b"partner-join");
        }
        if !sleep_preserves_if(true) {
            return fail(b"if-sleep-on");
        }
        if !sleep_preserves_if(false) {
            return fail(b"if-sleep-off");
        }
        if !event_preserves_if(true) {
            return fail(b"if-event-on");
        }
        if !event_preserves_if(false) {
            return fail(b"if-event-off");
        }
        puts(b"SYNCCHECK OK mutex_elapsed=");
        crate::boot::log_route::dec(elapsed);
        puts(b" event_iters=");
        crate::boot::log_route::dec(u64::from(EVENT_ITERS));
        puts(b" if_preserve=on/off\r\n");
        true
    }
}

unsafe fn kernel_write(text: *const u8, len: usize) {
    unsafe { crate::boot::log_route::r4os_rust_log_write(0, text, len) }
}
