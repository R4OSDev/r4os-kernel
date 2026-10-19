//! Existing synthetic backend and requester bodies; no physical device access.
use super::*;
pub(super) unsafe extern "C" fn slow_read(
    _: *mut c_void,
    _: u64,
    _: u16,
    out: *mut u8,
    len: usize,
) -> bool {
    let _activity = Activity::enter();
    signal(0);
    if wait(1, u64::MAX) != 1 {
        return false;
    }
    unsafe {
        out.write_bytes(0x53, len);
    }
    true
}
pub(super) unsafe extern "C" fn fast_read(
    _: *mut c_void,
    _: u64,
    _: u16,
    out: *mut u8,
    len: usize,
) -> bool {
    let _activity = Activity::enter();
    unsafe {
        out.write_bytes(0x46, len);
    }
    true
}
pub(super) unsafe extern "C" fn legacy_read(
    context: *mut c_void,
    lba: u64,
    sectors: u16,
    out: *mut u8,
    len: usize,
) -> bool {
    LEGACY_CPU.store(crate::arch::percpu::current_index(), Ordering::Release);
    unsafe { fast_read(context, lba, sectors, out, len) }
}
pub(super) unsafe extern "C" fn slow_request() {
    let Some(_active) = Requester::enter() else {
        return;
    };
    set(0, read(SLOW.load(Ordering::Acquire), 0, 0));
    signal(2);
}
pub(super) unsafe extern "C" fn fast_request() {
    let Some(_active) = Requester::enter() else {
        return;
    };
    set(1, read(FAST.load(Ordering::Acquire), 0, 1));
    signal(3);
}
pub(super) unsafe extern "C" fn legacy_submit(
    context: *mut c_void,
    input: *const device::AsyncRequest,
) -> i32 {
    let result = unsafe { submit(context, input) };
    if result != 0 {
        return result;
    }
    let r = unsafe { input.read() };
    if r.lba == 0 {
        let ok = unsafe { legacy_read(context, r.lba, r.sectors, r.buffer, r.buffer_len) };
        unsafe { (r.complete)(r.handle, if ok { 0 } else { -1 }, r.buffer_len as u32) };
        PENDING.fetch_and(!1, Ordering::Release);
    }
    0
}
pub(super) unsafe extern "C" fn legacy_cancel(
    context: *mut c_void,
    handle: u64,
    reason: u32,
) -> i32 {
    LEGACY_CANCEL_CPU.store(crate::arch::percpu::current_index(), Ordering::Release);
    unsafe { cancel(context, handle, reason) }
}
pub(super) unsafe extern "C" fn legacy_timeout() {
    let Some(_active) = Requester::enter() else {
        return;
    };
    LEGACY_CALLER_CPU.store(crate::arch::percpu::current_index(), Ordering::Release);
    set(1, read(FAST.load(Ordering::Acquire), 3, 1));
    signal(3);
}

pub(super) unsafe extern "C" fn no_sync(
    _: *mut c_void,
    _: u64,
    _: u16,
    _: *mut u8,
    _: usize,
) -> bool {
    false
}
pub(super) unsafe extern "C" fn submit(
    context: *mut c_void,
    request: *const device::AsyncRequest,
) -> i32 {
    if context != context_pointer() {
        return -1;
    }
    let n = SUBMISSIONS.load(Ordering::Acquire) as usize;
    if n >= 12 {
        return -1;
    }
    unsafe {
        REQUESTS.0[n].get().write(MaybeUninit::new(request.read()));
    }
    PENDING.fetch_or(1 << n, Ordering::Release);
    SUBMISSIONS.store(n as u32 + 1, Ordering::Release);
    signal(4);
    0
}
pub(super) fn request(index: usize) -> device::AsyncRequest {
    assert!(index < SUBMISSIONS.load(Ordering::Acquire) as usize);
    unsafe { (*REQUESTS.0[index].get()).assume_init() }
}
pub(super) fn find(kind: u8, lba: u64) -> Option<device::AsyncRequest> {
    (0..SUBMISSIONS.load(Ordering::Acquire) as usize)
        .map(request)
        .find(|r| r.kind == kind && r.lba == lba)
}
pub(super) unsafe extern "C" fn cancel(context: *mut c_void, handle: u64, reason: u32) -> i32 {
    if context != context_pointer() {
        return -1;
    }
    let Some(n) =
        (0..SUBMISSIONS.load(Ordering::Acquire) as usize).find(|&n| request(n).handle == handle)
    else {
        return -1;
    };
    CANCELS.fetch_add(1, Ordering::Relaxed);
    if reason == 1 {
        TIMEOUT_CANCELS.fetch_add(1, Ordering::Relaxed);
        for n in 0..SUBMISSIONS.load(Ordering::Acquire) as usize {
            let r = request(n);
            if r.handle == handle {
                unsafe {
                    (r.complete)(handle, -2, 0);
                    PENDING.fetch_and(!(1 << n), Ordering::Release);
                }
                return 0;
            }
        }
        return -1;
    }
    if reason == 3 {
        RESET_CANCELS.fetch_add(1, Ordering::Relaxed);
        PENDING.fetch_and(!(1 << n), Ordering::Release);
    }
    0
}
pub(super) unsafe extern "C" fn reset(context: *mut c_void, _: u32) -> i32 {
    if context != context_pointer() {
        return -1;
    }
    RESETS.fetch_add(1, Ordering::Relaxed);
    0
}
pub(super) fn wait_submissions(expected: u32) -> bool {
    while SUBMISSIONS.load(Ordering::Acquire) < expected {
        if !done(4) {
            return false;
        }
    }
    SUBMISSIONS.load(Ordering::Acquire) == expected
}
pub(super) unsafe extern "C" fn first() {
    let Some(_active) = Requester::enter() else {
        return;
    };
    set(2, read(ASYNC.load(Ordering::Acquire), 0, 2));
    signal(5);
}
pub(super) unsafe extern "C" fn second() {
    let Some(_active) = Requester::enter() else {
        return;
    };
    set(3, read(ASYNC.load(Ordering::Acquire), 1, 3));
    signal(6);
}
pub(super) unsafe extern "C" fn resetting() {
    let Some(_active) = Requester::enter() else {
        return;
    };
    set(4, read(ASYNC.load(Ordering::Acquire), 2, 4));
    signal(7);
}
pub(super) unsafe extern "C" fn timeout() {
    let Some(_active) = Requester::enter() else {
        return;
    };
    set(5, read(ASYNC.load(Ordering::Acquire), 3, 5));
    signal(8);
}
pub(super) unsafe extern "C" fn flush_first() {
    let Some(_active) = Requester::enter() else {
        return;
    };
    set(6, read(ASYNC.load(Ordering::Acquire), 4, 6));
    signal(9);
}
pub(super) unsafe extern "C" fn flush() {
    let Some(_active) = Requester::enter() else {
        return;
    };
    set(
        7,
        reference(ASYNC.load(Ordering::Acquire)).is_some_and(io::flush_exact),
    );
    signal(10);
}
pub(super) unsafe extern "C" fn flush_last() {
    let Some(_active) = Requester::enter() else {
        return;
    };
    set(8, read(ASYNC.load(Ordering::Acquire), 5, 7));
    signal(11);
}
pub(super) unsafe extern "C" fn batch() {
    let Some(_active) = Requester::enter() else {
        return;
    };
    let index = ASYNC.load(Ordering::Acquire);
    unsafe {
        buffer(2).write_bytes(0xe1, 512);
        buffer(3).write_bytes(0xe2, 512);
    }
    if let Some(expected) = reference(index) {
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
            io::r4os_rust_block_write_batch(index, &expected, writes.as_mut_ptr(), 2, true)
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
pub(super) fn complete(r: device::AsyncRequest, pattern: u8) {
    if !r.buffer.is_null() {
        unsafe {
            r.buffer.write_bytes(pattern, r.buffer_len);
        }
    }
    let _ = completion(r, 0, r.buffer_len as u32);
}

pub(super) fn completion(
    r: device::AsyncRequest,
    result: i32,
    bytes: u32,
) -> execution::CompletionReceipt {
    assert!(core::ptr::fn_addr_eq(
        r.complete,
        execution::r4os_rust_block_complete as unsafe extern "C" fn(u64, i32, u32)
    ));
    let receipt = unsafe { execution::complete_receipt(r.handle, result, bytes) };
    if receipt == execution::CompletionReceipt::Accepted {
        ACCEPTED.fetch_add(1, Ordering::Relaxed);
    }
    for n in 0..SUBMISSIONS.load(Ordering::Acquire) as usize {
        if request(n).handle == r.handle {
            PENDING.fetch_and(!(1 << n), Ordering::Release);
            break;
        }
    }
    receipt
}
