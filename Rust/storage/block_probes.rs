//! The original optional synchronous/async dispatch boot checks. Requests and
//! buffers remain resident through their actual completion/cancel acknowledgments.
use super::{
    block_accounting as accounting, block_device as device, block_execution as execution,
    block_io as io, block_registry as registry, block_workers as workers,
};
use crate::{
    boot::log_route::{dec, puts},
    sched::{scheduler, sync_objects, sync_types::Event, task::task_api},
};
use core::{
    cell::UnsafeCell,
    ffi::c_void,
    mem::MaybeUninit,
    ptr,
    sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
};
const WAIT: u64 = 2 * crate::arch::pit::DEFAULT_HZ as u64;
struct Events([UnsafeCell<Event>; 13]);
// Every event is mutated solely by the native generation-safe event owner.
unsafe impl Sync for Events {}
static EVENTS: Events = Events([const { UnsafeCell::new(Event::AUTO) }; 13]);
struct Buffers([UnsafeCell<[u8; 512]>; 8]);
// Each requester owns one buffer until its done event; the synthetic backend
// accesses a buffer only after request publication and before its completion.
unsafe impl Sync for Buffers {}
static BUFFERS: Buffers = Buffers([const { UnsafeCell::new([0; 512]) }; 8]);
struct Requests([UnsafeCell<MaybeUninit<device::AsyncRequest>>; 12]);
// The synthetic backend publishes each initialized slot through SUBMISSIONS;
// the BSP reads only that published prefix and never resets an active run.
unsafe impl Sync for Requests {}
static REQUESTS: Requests = Requests([const { UnsafeCell::new(MaybeUninit::uninit()) }; 12]);
static SUBMISSIONS: AtomicU32 = AtomicU32::new(0);
static CANCELS: AtomicU32 = AtomicU32::new(0);
static TIMEOUT_CANCELS: AtomicU32 = AtomicU32::new(0);
static RESET_CANCELS: AtomicU32 = AtomicU32::new(0);
static RESETS: AtomicU32 = AtomicU32::new(0);
static FLAGS: [AtomicBool; 10] = [const { AtomicBool::new(false) }; 10];
static SLOW: AtomicUsize = AtomicUsize::new(0);
static FAST: AtomicUsize = AtomicUsize::new(0);
static ASYNC: AtomicUsize = AtomicUsize::new(0);
fn signal(index: usize) {
    unsafe {
        sync_objects::r4os_rust_sync_event(EVENTS.0[index].get(), 0, 0);
    }
}
fn wait(index: usize, ticks: u64) -> u32 {
    unsafe { sync_objects::r4os_rust_sync_event(EVENTS.0[index].get(), 2, ticks) }
}
fn done(index: usize) -> bool {
    wait(index, WAIT) == 1
}
fn flag(index: usize) -> bool {
    FLAGS[index].load(Ordering::Acquire)
}
fn set(index: usize, value: bool) {
    FLAGS[index].store(value, Ordering::Release);
}
fn buffer(index: usize) -> *mut u8 {
    BUFFERS.0[index].get().cast()
}
fn summary() -> accounting::RuntimeSummary {
    let mut s = MaybeUninit::uninit();
    unsafe {
        workers::r4os_rust_block_workers_summary(s.as_mut_ptr());
        s.assume_init()
    }
}
fn read(index: usize, lba: u64, b: usize) -> bool {
    unsafe { io::r4os_rust_block_read(index, lba, 1, buffer(b), 512, true) }
}
fn queue(index: usize) -> u32 {
    unsafe { execution::r4os_rust_block_device_operation(index, 0, 0, 0, 0, 0) }
}
fn unregister(index: usize) -> bool {
    unsafe { execution::r4os_rust_block_unregister(index) }
}
fn active(index: usize) -> Option<u32> {
    let _owner = crate::memory::owner::Guard::acquire(1);
    let mut v = MaybeUninit::uninit();
    if !unsafe { registry::r4os_rust_block_registry_slot(index, 1, 0, true, v.as_mut_ptr()) } {
        return None;
    }
    let v = unsafe { v.assume_init() };
    Some(unsafe { ptr::addr_of!((*v.device.cast::<device::Device>()).active).read() })
}
fn create(name: &'static [u8], entry: unsafe extern "C" fn()) -> *mut c_void {
    let r = task_api::CreateRequest {
        name: name.as_ptr(),
        name_len: name.len(),
        state: 1,
        role: 3,
        needs_fpu: true,
        smp_eligible: false,
        smp_r4x: false,
        owner_kind: 0,
        owner_context: ptr::null_mut(),
        entry: Some(entry),
    };
    let mut failure = 0;
    unsafe { task_api::r4os_rust_task_create(&r, false, &mut failure) }
}
fn config(name: &'static [u8], controller: &'static [u8], read: device::Read) -> device::Config {
    let mut c = device::Config::new(device::text(name), read);
    c.controller = device::text(controller);
    c.bus = 5;
    c.sector_size = 512;
    c.sector_count = 1;
    c
}
fn sync_result(
    ok: bool,
    before: &accounting::RuntimeSummary,
    after: &accounting::RuntimeSummary,
) -> bool {
    puts(b"BLOCKDISPATCHCHECK ");
    puts(if ok { b"OK" } else { b"FAIL" });
    puts(b" controllers=");
    dec(u64::from(after.controller_count));
    puts(b" workers=");
    dec(u64::from(after.worker_count));
    puts(b" parallelMax=");
    dec(u64::from(after.worker_parallel_active_max));
    puts(b" directDelta=");
    dec(after.direct_requests.saturating_sub(before.direct_requests));
    puts(b" bounceAllocDelta=");
    dec(after
        .bounce_allocations
        .saturating_sub(before.bounce_allocations));
    puts(b" bounceCopyDelta=");
    dec(after
        .bounce_copy_bytes
        .saturating_sub(before.bounce_copy_bytes));
    puts(b"\r\n");
    ok
}
unsafe extern "C" fn slow_read(_: *mut c_void, _: u64, _: u16, out: *mut u8, len: usize) -> bool {
    signal(0);
    if wait(1, u64::MAX) != 1 {
        return false;
    }
    unsafe {
        out.write_bytes(0x53, len);
    }
    true
}
unsafe extern "C" fn fast_read(_: *mut c_void, _: u64, _: u16, out: *mut u8, len: usize) -> bool {
    unsafe {
        out.write_bytes(0x46, len);
    }
    true
}
unsafe extern "C" fn slow_request() {
    set(0, read(SLOW.load(Ordering::Acquire), 0, 0));
    signal(2);
    scheduler::exit_current(false);
}
unsafe extern "C" fn fast_request() {
    set(1, read(FAST.load(Ordering::Acquire), 0, 1));
    signal(3);
    scheduler::exit_current(false);
}

pub(crate) fn parallel_dispatch() -> bool {
    if !workers::r4os_rust_block_workers_ready() {
        return sync_result(
            false,
            &accounting::RuntimeSummary::default(),
            &accounting::RuntimeSummary::default(),
        );
    }
    for i in 0..4 {
        unsafe {
            EVENTS.0[i].get().write(Event::AUTO);
        }
    }
    set(0, false);
    set(1, false);
    unsafe {
        buffer(0).write_bytes(0, 512);
        buffer(1).write_bytes(0, 512);
    }
    let before = summary();
    let Some(slow) = (unsafe {
        device::register(config(
            b"dispatch-test-slow",
            b"dispatch-test-controller-slow",
            slow_read,
        ))
    }) else {
        return sync_result(false, &before, &summary());
    };
    SLOW.store(slow, Ordering::Release);
    let Some(fast) = (unsafe {
        device::register(config(
            b"dispatch-test-fast",
            b"dispatch-test-controller-fast",
            fast_read,
        ))
    }) else {
        unregister(slow);
        return sync_result(false, &before, &summary());
    };
    FAST.store(fast, Ordering::Release);
    let mut ok = true;
    let mut slow_started = false;
    let mut fast_started = false;
    let mut fast_observed = false;
    if create(b"blk-test-slow", slow_request).is_null() {
        ok = false;
    } else {
        slow_started = true;
    }
    if slow_started {
        fast_started = done(0) && !create(b"blk-test-fast", fast_request).is_null();
        ok = fast_started && ok;
    }
    if fast_started && done(3) {
        fast_observed = true;
    } else if fast_started {
        ok = false;
    }
    signal(1);
    if slow_started && !done(2) {
        ok = false;
    }
    if fast_started && !fast_observed && !done(3) {
        ok = false;
    }
    let after = summary();
    ok = flag(0)
        && flag(1)
        && after.controller_count >= before.controller_count + 2
        && after.worker_count >= before.worker_count + 2
        && after.worker_parallel_active_max >= 2
        && after.direct_requests >= before.direct_requests + 2
        && after.bounce_allocations == before.bounce_allocations
        && after.bounce_copy_bytes == before.bounce_copy_bytes
        && ok;
    let f = unregister(fast);
    let s = unregister(slow);
    ok = f && s && ok;
    let sync = sync_result(ok, &before, &after);
    let asynchronous = async_dispatch();
    sync && asynchronous
}
unsafe extern "C" fn no_sync(_: *mut c_void, _: u64, _: u16, _: *mut u8, _: usize) -> bool {
    false
}
unsafe extern "C" fn submit(context: *mut c_void, request: *const device::AsyncRequest) -> i32 {
    if context.is_null() {
        return -1;
    }
    let n = SUBMISSIONS.load(Ordering::Acquire) as usize;
    if n >= 12 {
        return -1;
    }
    unsafe {
        REQUESTS.0[n].get().write(MaybeUninit::new(request.read()));
    }
    SUBMISSIONS.store(n as u32 + 1, Ordering::Release);
    signal(4);
    0
}
fn request(index: usize) -> device::AsyncRequest {
    assert!(index < SUBMISSIONS.load(Ordering::Acquire) as usize);
    unsafe { (*REQUESTS.0[index].get()).assume_init() }
}
fn find(kind: u8, lba: u64) -> Option<device::AsyncRequest> {
    (0..SUBMISSIONS.load(Ordering::Acquire) as usize)
        .map(request)
        .find(|r| r.kind == kind && r.lba == lba)
}
unsafe extern "C" fn cancel(context: *mut c_void, handle: u64, reason: u32) -> i32 {
    if context.is_null() {
        return -1;
    }
    CANCELS.fetch_add(1, Ordering::Relaxed);
    if reason == 1 {
        TIMEOUT_CANCELS.fetch_add(1, Ordering::Relaxed);
        for n in 0..SUBMISSIONS.load(Ordering::Acquire) as usize {
            let r = request(n);
            if r.handle == handle {
                unsafe {
                    (r.complete)(handle, -2, 0);
                }
                return 0;
            }
        }
        return -1;
    }
    if reason == 3 {
        RESET_CANCELS.fetch_add(1, Ordering::Relaxed);
    }
    0
}
unsafe extern "C" fn reset(context: *mut c_void, _: u32) -> i32 {
    if context.is_null() {
        return -1;
    }
    RESETS.fetch_add(1, Ordering::Relaxed);
    0
}
fn wait_submissions(expected: u32) -> bool {
    while SUBMISSIONS.load(Ordering::Acquire) < expected {
        if !done(4) {
            return false;
        }
    }
    SUBMISSIONS.load(Ordering::Acquire) == expected
}
unsafe extern "C" fn first() {
    set(2, read(ASYNC.load(Ordering::Acquire), 0, 2));
    signal(5);
    scheduler::exit_current(false);
}
unsafe extern "C" fn second() {
    set(3, read(ASYNC.load(Ordering::Acquire), 1, 3));
    signal(6);
    scheduler::exit_current(false);
}
unsafe extern "C" fn resetting() {
    set(4, read(ASYNC.load(Ordering::Acquire), 2, 4));
    signal(7);
    scheduler::exit_current(false);
}
unsafe extern "C" fn timeout() {
    set(5, read(ASYNC.load(Ordering::Acquire), 3, 5));
    signal(8);
    scheduler::exit_current(false);
}
unsafe extern "C" fn flush_first() {
    set(6, read(ASYNC.load(Ordering::Acquire), 4, 6));
    signal(9);
    scheduler::exit_current(false);
}
unsafe extern "C" fn flush() {
    set(7, io::r4os_rust_block_flush(ASYNC.load(Ordering::Acquire)));
    signal(10);
    scheduler::exit_current(false);
}
unsafe extern "C" fn flush_last() {
    set(8, read(ASYNC.load(Ordering::Acquire), 5, 7));
    signal(11);
    scheduler::exit_current(false);
}
unsafe extern "C" fn batch() {
    let index = ASYNC.load(Ordering::Acquire);
    unsafe {
        buffer(2).write_bytes(0xe1, 512);
        buffer(3).write_bytes(0xe2, 512);
    }
    let mut limits = MaybeUninit::uninit();
    if unsafe { registry::r4os_rust_block_registry_limits(index, limits.as_mut_ptr()) } {
        let limits = unsafe { limits.assume_init() };
        let mut writes = [
            io::Write {
                lba: 6,
                sectors: 1,
                data: buffer(2),
                len: 512,
                attempted: false,
                ok: false,
            },
            io::Write {
                lba: 7,
                sectors: 1,
                data: buffer(3),
                len: 512,
                attempted: false,
                ok: false,
            },
        ];
        let pending = unsafe {
            io::r4os_rust_block_write_batch(index, &limits.reference, writes.as_mut_ptr(), 2, true)
        };
        set(
            9,
            pending == 2
                && writes[0].attempted
                && writes[1].attempted
                && writes[0].ok
                && !writes[1].ok
                && queue(index) == 0,
        );
    }
    signal(12);
}
fn complete(r: device::AsyncRequest, pattern: u8) {
    if !r.buffer.is_null() {
        unsafe {
            r.buffer.write_bytes(pattern, r.buffer_len);
        }
    }
    unsafe {
        (r.complete)(r.handle, 0, r.buffer_len as u32);
    }
}
fn async_result(ok: bool, b: &accounting::RuntimeSummary, a: &accounting::RuntimeSummary) -> bool {
    puts(b"BLOCKASYNCCHECK ");
    puts(if ok { b"OK" } else { b"FAIL" });
    puts(b" inflightMax=");
    dec(u64::from(a.in_flight_high_water));
    for (label, value) in [
        (
            b" submits=".as_slice(),
            a.async_submissions.saturating_sub(b.async_submissions),
        ),
        (
            b" completions=",
            a.async_completions.saturating_sub(b.async_completions),
        ),
        (
            b" duplicates=",
            a.duplicate_completions
                .saturating_sub(b.duplicate_completions),
        ),
        (
            b" late=",
            a.late_completions.saturating_sub(b.late_completions),
        ),
        (b" resets=", a.async_resets.saturating_sub(b.async_resets)),
        (
            b" cancels=",
            a.async_cancel_requests
                .saturating_sub(b.async_cancel_requests),
        ),
    ] {
        puts(label);
        dec(value);
    }
    puts(b"\r\n");
    ok
}
fn async_dispatch() -> bool {
    for c in [
        &SUBMISSIONS,
        &CANCELS,
        &TIMEOUT_CANCELS,
        &RESET_CANCELS,
        &RESETS,
    ] {
        c.store(0, Ordering::Release);
    }
    for i in 4..13 {
        unsafe {
            EVENTS.0[i].get().write(Event::AUTO);
        }
    }
    for i in 2..10 {
        set(i, i == 4 || i == 5);
    }
    for i in 2..8 {
        unsafe {
            buffer(i).write_bytes(0, 512);
        }
    }
    let before = summary();
    let mut c = config(
        b"dispatch-test-async",
        b"dispatch-test-controller-async",
        no_sync,
    );
    c.sector_count = 8;
    c.writable = true;
    c.queue_depth = 2;
    c.timeout = 50;
    c.context = ptr::addr_of!(SUBMISSIONS).cast_mut().cast();
    c.submit = Some(submit);
    c.cancel = Some(cancel);
    c.reset = Some(reset);
    let Some(index) = (unsafe { device::register(c) }) else {
        return async_result(false, &before, &summary());
    };
    ASYNC.store(index, Ordering::Release);
    let mut ok = true;
    if create(b"blk-async-a", first).is_null() || create(b"blk-async-b", second).is_null() {
        ok = false;
    }
    if ok && !wait_submissions(2) {
        ok = false;
    }
    if ok && active(index) != Some(2) {
        ok = false;
    }
    if ok {
        let Some(b) = find(1, 1) else {
            return async_result(false, &before, &summary());
        };
        let Some(a) = find(1, 0) else {
            return async_result(false, &before, &summary());
        };
        // Preserve the original out-of-order and duplicate completion sequence.
        if !b.buffer.is_null() {
            unsafe {
                b.buffer.write_bytes(0xb2, b.buffer_len);
            }
        }
        if !a.buffer.is_null() {
            unsafe {
                a.buffer.write_bytes(0xa1, a.buffer_len);
            }
        }
        unsafe {
            (b.complete)(b.handle, 0, b.buffer_len as u32);
            (b.complete)(b.handle, 0, b.buffer_len as u32);
            (a.complete)(a.handle, 0, a.buffer_len as u32);
        }
    }
    if !done(5) || !done(6) {
        ok = false;
    }
    ok =
        flag(2) && flag(3) && unsafe { buffer(2).read() == 0xa1 && buffer(3).read() == 0xb2 } && ok;
    let task = create(b"blk-async-reset", resetting);
    if task.is_null() {
        ok = false;
    }
    if ok && !wait_submissions(3) {
        ok = false;
    }
    let mut late_handle = 0;
    if ok {
        let Some(r) = find(1, 2) else {
            return async_result(false, &before, &summary());
        };
        late_handle = r.handle;
        let before_kill = unsafe { task_api::r4os_rust_task_control(12, 0, 0) };
        let id = unsafe { task_api::r4os_rust_task_field(task, 23) } as u32;
        if unsafe { task_api::r4os_rust_task_identity(2, id, 0, ptr::null_mut(), 0) } != 0
            || unsafe { task_api::r4os_rust_task_control(12, 0, 0) } <= before_kill
        {
            ok = false;
        }
        if unregister(index) {
            ok = false;
        }
        if !unsafe { execution::r4os_rust_block_reset(index, 3) } {
            ok = false;
        }
    }
    if !done(7) {
        ok = false;
    }
    if late_handle != 0 {
        unsafe {
            execution::r4os_rust_block_complete(late_handle, 0, 512);
        }
    }
    if create(b"blk-async-timeout", timeout).is_null() {
        ok = false;
    }
    if ok && !wait_submissions(4) {
        ok = false;
    }
    if !done(8) {
        ok = false;
    }
    ok = !flag(5) && TIMEOUT_CANCELS.load(Ordering::Acquire) == 1 && ok;
    if create(b"blk-async-before-flush", flush_first).is_null() {
        ok = false;
    }
    if ok && !wait_submissions(5) {
        ok = false;
    }
    if create(b"blk-async-flush", flush).is_null() {
        ok = false;
    }
    if ok && (wait(4, 5) != 2 || queue(index) != 2) {
        ok = false;
    }
    if create(b"blk-async-after-flush", flush_last).is_null() {
        ok = false;
    }
    if ok && (wait(4, 5) != 2 || queue(index) != 3) {
        ok = false;
    }
    if ok {
        let r = request(4);
        if r.kind != 1 || r.lba != 4 {
            ok = false;
        } else {
            complete(r, 0xc4);
        }
    }
    if ok && !wait_submissions(6) {
        ok = false;
    }
    if ok {
        let r = request(5);
        if r.kind != 3 || SUBMISSIONS.load(Ordering::Acquire) != 6 {
            ok = false;
        } else {
            unsafe {
                (r.complete)(r.handle, 0, 0);
            }
        }
    }
    if ok && !wait_submissions(7) {
        ok = false;
    }
    if ok {
        let r = request(6);
        if r.kind != 1 || r.lba != 5 {
            ok = false;
        } else {
            complete(r, 0xd5);
        }
    }
    if !done(9) || !done(10) || !done(11) {
        ok = false;
    }
    ok = flag(6)
        && flag(7)
        && flag(8)
        && unsafe { buffer(6).read() == 0xc4 && buffer(7).read() == 0xd5 }
        && ok;
    if create(b"blk-write-batch", batch).is_null() {
        ok = false;
    }
    if ok && !wait_submissions(9) {
        ok = false;
    }
    if ok {
        let a = request(7);
        let b = request(8);
        if a.kind != 2
            || b.kind != 2
            || a.const_buffer.is_null()
            || b.const_buffer.is_null()
            || unsafe { a.const_buffer.read() != 0xe1 || b.const_buffer.read() != 0xe2 }
            || active(index) != Some(2)
            || unregister(index)
        {
            ok = false;
        }
        unsafe {
            (b.complete)(b.handle, -1, 0);
            (a.complete)(a.handle, 0, a.buffer_len as u32);
        }
    }
    if !done(12) {
        ok = false;
    }
    ok = flag(9) && ok;
    puts(if flag(9) {
        b"[BLOCKWRITEBATCH] result=OK pending=2 partial_failure=1 drained=1\r\n"
    } else {
        b"[BLOCKWRITEBATCH] result=FAIL\r\n"
    });
    let after = summary();
    ok = !flag(4)
        && CANCELS.load(Ordering::Acquire) == 2
        && RESET_CANCELS.load(Ordering::Acquire) == 1
        && RESETS.load(Ordering::Acquire) == 1
        && after.in_flight_high_water >= 2
        && after.duplicate_completions > before.duplicate_completions
        && after.late_completions > before.late_completions
        && ok;
    ok = unregister(index) && ok;
    async_result(ok, &before, &after)
}
