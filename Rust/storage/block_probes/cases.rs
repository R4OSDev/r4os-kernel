//! Existing scenario ordering and payload assertions, under one owned session.
use super::backend::*;
use super::*;
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
pub(super) fn run() -> bool {
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
        register(config(
            b"dispatch-test-slow",
            b"dispatch-test-controller-slow",
            slow_read,
        ))
    }) else {
        return sync_result(false, &before, &summary());
    };
    SLOW.store(slow, Ordering::Release);
    let Some(fast) = (unsafe {
        register(config(
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
    let sync = sync_result(ok && finish_workers(), &before, &after);
    if !case(1, sync) {
        return false;
    }
    let asynchronous = async_dispatch();
    sync && asynchronous
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
    let Some(index) = (unsafe { register(c) }) else {
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
    if !case(2, ok) {
        return async_result(false, &before, &summary());
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
        {
            let first = completion(b, 0, b.buffer_len as u32);
            let replay = completion(b, 0, b.buffer_len as u32);
            let last = completion(a, 0, a.buffer_len as u32);
            DUPLICATE_KIND.store(
                match replay {
                    execution::CompletionReceipt::Accepted => 1,
                    execution::CompletionReceipt::Duplicate => 2,
                    execution::CompletionReceipt::Late => 3,
                },
                Ordering::Release,
            );
            if replay != execution::CompletionReceipt::Accepted {
                DUPLICATE_REJECTED.fetch_add(1, Ordering::Relaxed);
            }
            ok = first == execution::CompletionReceipt::Accepted
                && last == execution::CompletionReceipt::Accepted
                && replay != execution::CompletionReceipt::Accepted
                && ok;
        }
    }
    if !done(5) || !done(6) {
        ok = false;
    }
    ok =
        flag(2) && flag(3) && unsafe { buffer(2).read() == 0xa1 && buffer(3).read() == 0xb2 } && ok;
    if !case(4, ok) {
        return async_result(false, &before, &summary());
    }
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
        ok = kill_denied(task) && ok;
        if unregister(index) {
            ok = false;
        }
        if !case(8, ok) {
            return async_result(false, &before, &summary());
        }
        if !reference(index).is_some_and(|v| unsafe { execution::reset_exact(v, 3) }) {
            ok = false;
        }
    }
    if !done(7) {
        ok = false;
    }
    if late_handle != 0 {
        let late = unsafe { execution::complete_receipt(late_handle, 0, 512) };
        if late == execution::CompletionReceipt::Late {
            LATE_REJECTED.fetch_add(1, Ordering::Relaxed);
        } else {
            ok = false;
        }
    }
    if !case(
        16,
        ok && !flag(4)
            && RESET_CANCELS.load(Ordering::Acquire) == 1
            && RESETS.load(Ordering::Acquire) == 1
            && LATE_REJECTED.load(Ordering::Acquire) == 1,
    ) {
        return async_result(false, &before, &summary());
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
    if !case(32, ok) {
        return async_result(false, &before, &summary());
    }
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
            {
                ok = completion(r, 0, 0) == execution::CompletionReceipt::Accepted && ok;
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
    if !case(64, ok) {
        return async_result(false, &before, &summary());
    }
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
        {
            ok = completion(b, -1, 0) == execution::CompletionReceipt::Accepted && ok;
            ok = completion(a, 0, a.buffer_len as u32) == execution::CompletionReceipt::Accepted
                && ok;
        }
    }
    if !done(12) {
        ok = false;
    }
    ok = flag(9) && ok;
    if !case(128, ok) {
        return async_result(false, &before, &summary());
    }
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
    ok = finish_workers() && ok;
    ok = unregister(index) && ok;
    async_result(case(128, ok), &before, &after)
}
