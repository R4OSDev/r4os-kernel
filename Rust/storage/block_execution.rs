//! Original device queue admission, physical backend execution and completion.
//! All mutable fields use resident raw C addresses. Queue mutex admission is
//! retained; storage guards close IRQ metadata races and never cross waits,
//! backend calls, wakeups, frees or callbacks that may re-enter block storage.
use crate::{
    arch::pit,
    memory::{heap, owner::Guard},
    platform::{clock_math, timer},
    sched::{
        scheduler, sync_locks,
        sync_types::{Mutex, Text, WaitQueue, WaitRequest},
        sync_wait, task_context,
    },
    storage::{
        block_accounting as accounting, block_device_state, block_diagnostics, block_io,
        block_registry, block_requests, block_state, block_workers, dispatch,
    },
};
use core::{
    ffi::c_void,
    mem::MaybeUninit,
    ptr,
    sync::atomic::{AtomicU64, Ordering},
};
const SLICE: u64 = 5 * pit::DEFAULT_HZ as u64;
// Resident wake epochs belong to the original 16 device slots. Reuse never
// resets them; a waiter always snapshots before checking its exact request.
// A wake before enrollment must force a retry, including a synchronous cancel
// callback whose completion has already been consumed by another CPU.
static WAKE_EPOCHS: [AtomicU64; 16] = [const { AtomicU64::new(0) }; 16];
#[derive(Clone, Copy)]
struct WakeEpoch {
    index: usize,
    observed: u64,
}
impl WakeEpoch {
    fn snapshot(v: View) -> Self {
        let index = usize::from(v.index);
        Self {
            index,
            observed: WAKE_EPOCHS[index].load(Ordering::Acquire),
        }
    }
}
unsafe extern "C" fn epoch_unchanged(context: *mut c_void) -> bool {
    let epoch = unsafe { &*context.cast::<WakeEpoch>() };
    WAKE_EPOCHS[epoch.index].load(Ordering::Acquire) == epoch.observed
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct View {
    pub io: block_io::View,
    pub state: block_device_state::DeviceState,
    pub slots: *mut block_requests::Slot,
    pub mutex: *mut Mutex,
    pub slot_available: *mut WaitQueue,
    pub completion_available: *mut WaitQueue,
    pub name: Text,
    pub driver: Text,
    pub index: u8,
    pub lane: u8,
    pub has_cancel: bool,
    pub has_reset: bool,
    pub bus: u8,
    pub queue_depth: *mut u16,
}

pub(crate) unsafe fn view(device: *mut c_void) -> View {
    let mut out = MaybeUninit::uninit();
    unsafe {
        block_execution_view(device, out.as_mut_ptr());
        out.assume_init()
    }
}
/// # Safety
/// Exact admitted device; output is exclusive C Stats storage. Preserve the
/// original per-device queue snapshot boundary before short metadata copying.
pub(crate) unsafe fn stats_snapshot(device: *mut c_void, out: *mut accounting::Stats) {
    let v = unsafe { view(device) };
    let _lock = unsafe { lock(device, v) };
    unsafe {
        accounting::r4os_rust_block_stats_snapshot(v.io.stats, out);
    }
}
struct DeviceLock {
    mutex: *mut Mutex,
    locked: bool,
}
impl Drop for DeviceLock {
    fn drop(&mut self) {
        if self.locked {
            unsafe {
                sync_locks::r4os_rust_sync_mutex(self.mutex, 2, 0);
            }
        }
    }
}
struct Incident(u64);
impl Drop for Incident {
    fn drop(&mut self) {
        if self.0 != 0 {
            unsafe {
                block_incident_resolve(self.0);
            }
        }
    }
}
unsafe fn lock(device: *mut c_void, v: View) -> DeviceLock {
    if scheduler::current().is_null() {
        return DeviceLock {
            mutex: v.mutex,
            locked: false,
        };
    }
    let mut slices = 0u32;
    let mut incident = Incident(0);
    loop {
        if unsafe { sync_locks::r4os_rust_sync_mutex(v.mutex, 1, SLICE) } {
            return DeviceLock {
                mutex: v.mutex,
                locked: true,
            };
        }
        slices += 1;
        unsafe {
            block_diagnostics::lock_report(device, slices, &mut incident.0);
        }
    }
}
unsafe fn wake(v: View, queue: *mut WaitQueue, all: bool) {
    // Publish before taking Runtime for wake: its wait predicate and actual
    // enrollment share that owner, so either the change or the wake is seen.
    WAKE_EPOCHS[usize::from(v.index)].fetch_add(1, Ordering::Release);
    unsafe {
        sync_wait::r4os_rust_sync_queue(queue, u8::from(all), ptr::null_mut(), 0, ptr::null_mut());
    }
}
unsafe fn queue_wait(
    queue: *mut WaitQueue,
    timeout: u64,
    reason: &'static [u8],
    mut epoch: WakeEpoch,
) -> u32 {
    let request = WaitRequest {
        timeout,
        reason: Text::literal(reason),
        predicate: Some(epoch_unchanged),
        context: ptr::addr_of_mut!(epoch).cast(),
        release: None,
        release_context: ptr::null_mut(),
        flags: 0,
        consume_runtime: false,
    };
    unsafe { sync_wait::r4os_rust_sync_wait(queue, &request) }
}
fn stat(v: View, operation: u8, field: u8, value: u64) -> u64 {
    unsafe { accounting::r4os_rust_block_stats_change(v.io.stats, operation, field, value) }
}
fn state(v: View, operation: u8, kind: u8, error: u8, sectors: u16) -> bool {
    unsafe {
        block_device_state::r4os_rust_block_device_state(&v.state, operation, kind, error, sectors)
    }
}
unsafe fn slot(v: View, index: usize) -> *mut block_requests::Slot {
    assert!(index < 16);
    unsafe { v.slots.add(index) }
}
unsafe fn select(v: View, operation: u8, id: u64) -> usize {
    unsafe { block_requests::r4os_rust_block_select(v.slots, operation, id) }
}
unsafe fn snapshot(p: *const block_requests::Slot) -> block_requests::Snapshot {
    let mut out = MaybeUninit::uninit();
    unsafe {
        block_requests::r4os_rust_block_snapshot(p, out.as_mut_ptr());
        out.assume_init()
    }
}
unsafe fn request_stats(v: View, last: bool, value: block_requests::Snapshot) {
    unsafe {
        accounting::r4os_rust_block_stats_request(v.io.stats, last, &value);
    }
}
fn timeout(v: View) -> u64 {
    if v.io.timeout == 0 {
        u64::MAX
    } else {
        v.io.timeout
    }
}
fn backpressure(device: *mut c_void, v: View, error: u8) {
    let _lock = unsafe { lock(device, v) };
    stat(v, 1, 7, 1);
    stat(v, 1, 2, 1);
    if error == 7 {
        stat(v, 1, 3, 1);
    }
    stat(v, 0, 42, u64::from(error));
}
/// # Safety
/// Exact pinned device and initialized C input/output remain resident through
/// waits. Buffer ownership was selected before admission; no buffer is freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_enqueue(
    device: *mut c_void,
    input: *const block_io::Input,
    out: *mut u64,
) -> bool {
    let v = unsafe { view(device) };
    let a = unsafe { input.read() };
    let budget = timeout(v);
    let forever = budget == u64::MAX;
    let start = timer::tick_count();
    let deadline = if forever {
        u64::MAX
    } else {
        clock_math::finite_deadline(start, budget)
    };
    loop {
        let epoch = WakeEpoch::snapshot(v);
        let owner = unsafe { lock(device, v) };
        if unsafe { v.state.resetting.read() } {
            drop(owner);
            backpressure(device, v, 1);
            return false;
        }
        let index = unsafe { select(v, 0, 0) };
        if index != usize::MAX {
            let id = stat(v, 1, 0, 1);
            let request = block_state::Execution {
                slot_index: index,
                id,
                kind: a.kind,
                lba: a.lba,
                sectors: a.sectors,
                buffer: a.buffer,
                const_buffer: a.data,
                buffer_len: a.len,
                buffer_ownership: a.ownership,
                start_tick: 0,
                mode: 0,
                backend_handle: 0,
                observation: a.observation,
            };
            unsafe {
                block_state::r4os_rust_block_slot_enqueue(
                    slot(v, index),
                    &request,
                    id,
                    timer::tick_count(),
                );
            }
            stat(v, 1, 4, 1);
            let used = unsafe { select(v, 3, 0) };
            stat(v, 5, 18, used as u64);
            unsafe {
                out.write(id);
            }
            return true;
        }
        stat(v, 1, 6, 1);
        drop(owner);
        if scheduler::current().is_null() {
            backpressure(device, v, 1);
            return false;
        }
        let now = timer::tick_count();
        if !forever && now >= deadline {
            backpressure(device, v, 7);
            return false;
        }
        let result = unsafe {
            queue_wait(
                v.slot_available,
                if forever { u64::MAX } else { deadline - now },
                b"block-slot",
                epoch,
            )
        };
        if result == 1 {
            continue;
        }
        backpressure(
            device,
            v,
            match result {
                2 => 7,
                3..=5 => 11,
                _ => 1,
            },
        );
        return false;
    }
}
unsafe fn begin(device: *mut c_void, v: View, mode: u8) -> Option<block_state::Execution> {
    let _lock = unsafe { lock(device, v) };
    let index = unsafe { select(v, 1, 0) };
    if index == usize::MAX {
        return None;
    }
    let p = unsafe { slot(v, index) };
    let allowed = {
        let _owner = Guard::acquire(1);
        !unsafe { v.state.resetting.read() }
            && dispatch::r4os_rust_storage_submission(
                unsafe { v.state.active.read() },
                v.io.queue_depth,
                unsafe { (*p).kind } == 3,
                unsafe { select(v, 4, 0) } != 0,
            )
    };
    if !allowed {
        return None;
    }
    let tick = timer::tick_count();
    let asynchronous = block_requests::r4os_rust_block_async_submission(mode, v.io.has_submit);
    let handle = if asynchronous {
        accounting::r4os_rust_block_next_handle(v.index, index)
    } else {
        0
    };
    let mut out = MaybeUninit::uninit();
    if !unsafe {
        block_state::r4os_rust_block_slot_start(
            p,
            index,
            mode,
            tick,
            asynchronous,
            handle,
            out.as_mut_ptr(),
        )
    } {
        return None;
    }
    state(v, 7, 0, 0, 0);
    accounting::r4os_rust_block_runtime_active(true);
    stat(v, 1, 5, 1);
    unsafe {
        request_stats(v, false, snapshot(p));
    }
    state(v, 11, 0, 0, 0);
    stat(v, 1, if mode == 0 { 16 } else { 14 }, 1);
    accounting::r4os_rust_block_runtime_execution(mode, false);
    Some(unsafe { out.assume_init() })
}
unsafe fn ready(
    device: *mut c_void,
    v: View,
) -> Option<(block_state::Execution, dispatch::Completion)> {
    let _lock = unsafe { lock(device, v) };
    for index in 0..16 {
        let p = unsafe { slot(v, index) };
        let mut completion = MaybeUninit::uninit();
        let available = {
            let _owner = Guard::acquire(1);
            unsafe {
                (*p).state == 2
                    && (*p).backend_handle != 0
                    && dispatch::r4os_rust_storage_latch(
                        ptr::addr_of_mut!((*p).completion_latch),
                        3,
                        0,
                        0,
                        0,
                        completion.as_mut_ptr(),
                    )
            }
        };
        if available {
            let mut request = MaybeUninit::uninit();
            unsafe {
                block_state::r4os_rust_block_execution(index, p, request.as_mut_ptr());
                return Some((request.assume_init(), completion.assume_init()));
            }
        }
    }
    None
}
unsafe fn finish(
    device: *mut c_void,
    v: View,
    request: block_state::Execution,
    ok: bool,
    error: u8,
) {
    let owner = unsafe { lock(device, v) };
    if request.slot_index >= 16 {
        return;
    }
    let p = unsafe { slot(v, request.slot_index) };
    let tick = timer::tick_count();
    let mut transition = MaybeUninit::uninit();
    if !unsafe {
        block_state::r4os_rust_block_slot_finish(
            p,
            &request,
            ok,
            error,
            tick,
            transition.as_mut_ptr(),
        )
    } {
        return;
    }
    let transition = unsafe { transition.assume_init() };
    state(v, 8, 0, 0, 0);
    accounting::r4os_rust_block_runtime_active(false);
    let latency = tick.saturating_sub(request.start_tick);
    unsafe {
        request_stats(v, true, snapshot(p));
    }
    let mut oldest = MaybeUninit::uninit();
    unsafe {
        block_requests::r4os_rust_block_oldest(v.slots, request.slot_index, oldest.as_mut_ptr());
        request_stats(v, false, oldest.assume_init());
    }
    stat(v, 0, 12, latency);
    stat(v, 1, 10, latency);
    stat(v, 5, 11, latency);
    if transition.completion_override != 0 {
        if !transition.caller_detached {
            state(v, 10, request.kind, transition.completion_override, 0);
        }
    } else if ok {
        state(v, 9, request.kind, 0, request.sectors);
    } else {
        state(v, 10, request.kind, error, 0);
    }
    if request.backend_handle != 0 {
        stat(v, 1, 36, 1);
        accounting::r4os_rust_block_runtime_change(1, 23, 1);
    }
    if !transition.caller_detached {
        unsafe {
            wake(v, v.completion_available, true);
        }
        stat(v, 1, 13, 1);
        accounting::r4os_rust_block_runtime_change(1, 10, 1);
    }
    stat(v, 1, if request.mode == 0 { 17 } else { 15 }, 1);
    accounting::r4os_rust_block_runtime_execution(request.mode, true);
    if transition.caller_detached {
        unsafe {
            block_state::r4os_rust_block_slot_clear(p);
            wake(v, v.slot_available, false);
        }
    }
    drop(owner);
    if !transition.detached_buffer.is_null() {
        unsafe {
            heap::r4os_rust_heap_free(transition.detached_buffer, transition.detached_len);
        }
    }
}
struct BackendUnwind(task_context::Unwind);
impl BackendUnwind {
    fn enter() -> Option<Self> {
        let token = unsafe { task_context::enter() };
        if token.admitted() {
            Some(Self(token))
        } else {
            None
        }
    }
}
impl Drop for BackendUnwind {
    fn drop(&mut self) {
        unsafe {
            task_context::leave(self.0);
        }
    }
}
unsafe fn execute(
    device: *mut c_void,
    v: View,
    request: block_state::Execution,
) -> block_requests::ResultRecord {
    let Some(_unwind) = BackendUnwind::enter() else {
        return block_requests::ResultRecord {
            err: 1,
            ..Default::default()
        };
    };
    let error = match request.kind {
        1 if request.buffer.is_null() => 5,
        2 if !v.io.has_writer => 6,
        2 if request.const_buffer.is_null() => 5,
        3 if !v.io.has_flush => 0,
        0 => 2,
        1..=3 => {
            if unsafe { crate::storage::block_device::backend(device, &request, request.kind, 0) }
                != 0
            {
                0
            } else {
                block_requests::r4os_rust_block_backend_error(request.kind)
            }
        }
        _ => unreachable!(),
    };
    block_requests::ResultRecord {
        ok: error == 0,
        err: error,
        buffer_detached: false,
    }
}
unsafe fn submit(device: *mut c_void, v: View, request: block_state::Execution) {
    stat(v, 1, 35, 1);
    accounting::r4os_rust_block_runtime_change(1, 22, 1);
    let Some(_unwind) = BackendUnwind::enter() else {
        unsafe {
            finish(device, v, request, false, 1);
        }
        return;
    };
    let result = unsafe { crate::storage::block_device::backend(device, &request, 4, 0) };
    if result == 0 {
        return;
    }
    let rejected = {
        let _owner = Guard::acquire(1);
        request.slot_index < 16
            && unsafe {
                dispatch::r4os_rust_storage_latch(
                    ptr::addr_of_mut!((*slot(v, request.slot_index)).completion_latch),
                    2,
                    request.backend_handle,
                    0,
                    0,
                    ptr::null_mut(),
                )
            }
    };
    if rejected {
        unsafe {
            finish(
                device,
                v,
                request,
                false,
                if result > 0 {
                    1
                } else {
                    block_requests::r4os_rust_block_backend_error(request.kind)
                },
            );
        }
    }
}
/// # Safety
/// Exact pinned device stays admitted through each backend and completion;
/// caller selected the execution mode before publishing any borrowed buffer.
pub unsafe fn pump(device: *mut c_void, mode: u8) -> bool {
    let Some(_unwind) = task_context::Admission::acquire(false) else {
        return false;
    };
    let v = unsafe { view(device) };
    let mut worked = false;
    let mut steps = 0;
    loop {
        // One busy device must not drain a continuously replenished queue
        // before the controller can visit its other devices. Inline boot work
        // still drains fully; it has no worker that could resume a short pass.
        if mode == 1 && steps == 8 {
            break;
        }
        if let Some((request, completion)) = unsafe { ready(device, v) } {
            worked = true;
            let mut result = MaybeUninit::uninit();
            unsafe {
                block_requests::r4os_rust_block_classify(
                    request.kind,
                    request.buffer_len,
                    completion.result,
                    completion.bytes,
                    result.as_mut_ptr(),
                );
            }
            let result = unsafe { result.assume_init() };
            unsafe {
                finish(device, v, request, result.ok, result.err);
            }
            steps += 1;
            // Exact completion, wakes and detached-buffer release are done;
            // the device pin and task unwind anchor remain, without a lock.
            scheduler::safe_reschedule();
            continue;
        }
        let pending_cancel = {
            let _owner = unsafe { lock(device, v) };
            unsafe { block_state::take_cancel(v.slots) }
        };
        if let Some((request, reason)) = pending_cancel {
            // Backend admission belongs to this exact controller worker. A
            // borrowed AP buffer/caller remains retained until real completion.
            unsafe { cancel(device, v, request, reason) };
            worked = true;
            steps += 1;
            scheduler::safe_reschedule();
            continue;
        }
        let Some(request) = (unsafe { begin(device, v, mode) }) else {
            break;
        };
        worked = true;
        if block_requests::r4os_rust_block_async_submission(mode, v.io.has_submit) {
            unsafe {
                submit(device, v, request);
            }
        } else {
            let result = unsafe { execute(device, v, request) };
            unsafe {
                finish(device, v, request, result.ok, result.err);
            }
        }
        steps += 1;
        // An asynchronous request is fully published, or inline completion is
        // acknowledged. Never suspend inside the backend/queue transition.
        scheduler::safe_reschedule();
    }
    worked
}
/// # Safety
/// Exact pinned device; worker decision is the one captured before admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_schedule(device: *mut c_void, worker: bool) {
    if worker {
        accounting::r4os_rust_block_runtime_change(1, 2, 1);
        let v = unsafe { view(device) };
        let lane = block_workers::r4os_rust_block_worker_lane(v.lane);
        if lane != 255 {
            block_workers::r4os_rust_block_worker_signal(lane);
            return;
        }
    }
    unsafe {
        pump(device, 0);
    }
}
const _: () = {
    assert!(core::mem::size_of::<View>() == 176);
};
enum Abort {
    Retry,
    Wait,
    Done(block_requests::ResultRecord),
}
fn result(error: u8, detached: bool) -> block_requests::ResultRecord {
    block_requests::ResultRecord {
        ok: false,
        err: error,
        buffer_detached: detached,
    }
}
unsafe fn cancel(device: *mut c_void, v: View, request: block_state::Execution, reason: u32) {
    if request.backend_handle == 0 || !v.has_cancel {
        return;
    }
    if !unsafe { crate::storage::block_device::callbacks_allowed(device) } {
        let pending = {
            let _owner = unsafe { lock(device, v) };
            unsafe { block_state::defer_cancel(slot(v, request.slot_index), request, reason) }
        };
        if pending {
            let lane = block_workers::r4os_rust_block_worker_lane(v.lane);
            if lane != 255 {
                block_workers::r4os_rust_block_worker_signal(lane);
            }
        }
        return;
    }
    stat(v, 1, 37, 1);
    accounting::r4os_rust_block_runtime_change(1, 24, 1);
    unsafe {
        crate::storage::block_device::backend(device, &request, 5, reason);
    }
}
unsafe fn abort(device: *mut c_void, v: View, id: u64, error: u8, reason: u32) -> Abort {
    let owner = unsafe { lock(device, v) };
    let index = unsafe { select(v, 2, id) };
    if index == usize::MAX {
        if error == 7 {
            stat(v, 1, 3, 1);
            stat(v, 1, 9, 1);
        }
        stat(v, 0, 42, u64::from(error));
        return Abort::Done(result(error, false));
    }
    let p = unsafe { slot(v, index) };
    let mut change = MaybeUninit::uninit();
    let mut request = MaybeUninit::uninit();
    unsafe {
        block_state::r4os_rust_block_slot_abort(
            p,
            error,
            change.as_mut_ptr(),
            request.as_mut_ptr(),
            index,
        );
    }
    let change = unsafe { change.assume_init() };
    match change.action {
        1 => {
            state(v, 10, change.kind, error, 0);
            if error == 7 {
                stat(v, 1, 9, 1);
            }
            unsafe {
                block_state::r4os_rust_block_slot_clear(p);
                wake(v, v.slot_available, false);
            }
            Abort::Done(result(error, false))
        }
        2 => Abort::Retry,
        3 => {
            if change.first_timeout {
                stat(v, 1, 9, 1);
                stat(v, 1, 34, 1);
                accounting::r4os_rust_block_runtime_change(1, 21, 1);
            }
            drop(owner);
            if change.first_cancel {
                unsafe {
                    cancel(device, v, request.assume_init(), reason);
                }
            }
            Abort::Wait
        }
        4 => {
            if change.first_timeout {
                stat(v, 1, 9, 1);
            }
            if change.first_detach {
                state(v, 10, change.kind, error, 0);
            }
            drop(owner);
            if change.first_cancel {
                unsafe {
                    cancel(device, v, request.assume_init(), reason);
                }
            }
            Abort::Done(result(error, change.buffer_detached))
        }
        0 => Abort::Done(result(error, false)),
        _ => unreachable!(),
    }
}
fn report_slice(slices: u32) -> bool {
    slices == 1 || slices.is_power_of_two()
}
/// # Safety
/// Exact pinned device/id; output is exclusive initialized C result storage.
/// Caller keeps bounce ownership/direct residency until actual completion or
/// acknowledged physical reset; no ownership is inferred from a timeout.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_wait(
    device: *mut c_void,
    id: u64,
    timeout: u64,
    out: *mut block_requests::ResultRecord,
) {
    let done = unsafe { wait(device, id, timeout) };
    unsafe {
        out.write(done);
    }
}
unsafe fn wait(device: *mut c_void, id: u64, timeout: u64) -> block_requests::ResultRecord {
    unsafe { wait_receipted(device, id, timeout, ptr::null_mut()) }
}
#[inline(always)]
pub(crate) unsafe fn wait_receipted(
    device: *mut c_void,
    id: u64,
    timeout: u64,
    receipt: *mut crate::storage::block_observation::ReadReceipt,
) -> block_requests::ResultRecord {
    let v = unsafe { view(device) };
    let mut counted = false;
    let mut slices = 0u32;
    let mut incident = Incident(0);
    let forever = timeout == u64::MAX;
    let start = timer::tick_count();
    let deadline = if forever {
        u64::MAX
    } else {
        clock_math::finite_deadline(start, timeout)
    };
    loop {
        let epoch = WakeEpoch::snapshot(v);
        let owner = unsafe { lock(device, v) };
        let index = unsafe { select(v, 2, id) };
        if index == usize::MAX {
            return result(2, false);
        }
        if !counted {
            stat(v, 1, 8, 1);
            counted = true;
        }
        let p = unsafe { slot(v, index) };
        let completed = {
            let _owner = Guard::acquire(1);
            if unsafe { (*p).state } == 3 {
                if !receipt.is_null() {
                    let mut flags = crate::contract::kernel_nvme_check_receipt_completed;
                    unsafe {
                        if (*p).execution_mode == 1 {
                            flags |= crate::contract::kernel_nvme_check_receipt_runtime;
                        }
                        if (*p).backend_handle != 0 {
                            flags |= crate::contract::kernel_nvme_check_receipt_asynchronous;
                        }
                        if !(*p).cancel_requested
                            && !(*p).timeout_requested
                            && (*p).completion_override == 0
                        {
                            flags |= crate::contract::kernel_nvme_check_receipt_clean;
                        }
                        receipt.write(crate::storage::block_observation::ReadReceipt {
                            flags,
                            reserved0: 0,
                            request_id: (*p).id,
                            backend_handle: (*p).backend_handle,
                            bytes: if (*p).ok { (*p).buffer_len as u64 } else { 0 },
                        });
                    }
                }
                Some(block_requests::ResultRecord {
                    ok: unsafe { (*p).ok },
                    err: unsafe { (*p).err },
                    buffer_detached: false,
                })
            } else {
                None
            }
        };
        if let Some(done) = completed {
            unsafe {
                block_state::r4os_rust_block_slot_clear(p);
                wake(v, v.slot_available, false);
            }
            return done;
        }
        drop(owner);
        if scheduler::current().is_null() {
            unsafe {
                pump(device, 0);
            }
            continue;
        }
        if forever {
            let wait =
                unsafe { queue_wait(v.completion_available, SLICE, b"block-completion", epoch) };
            if wait == 1 {
                continue;
            }
            if (3..=5).contains(&wait) {
                match unsafe { abort(device, v, id, 11, 2) } {
                    Abort::Retry => continue,
                    Abort::Done(done) => return done,
                    Abort::Wait => {}
                }
            }
            slices += 1;
            if report_slice(slices) {
                unsafe {
                    block_diagnostics::wait_report(device, id, slices, &mut incident.0);
                }
            }
            continue;
        }
        let now = timer::tick_count();
        let mut waited = 2;
        if now < deadline {
            waited = unsafe {
                queue_wait(
                    v.completion_available,
                    deadline - now,
                    b"block-completion",
                    epoch,
                )
            };
            if waited == 1 {
                continue;
            }
        }
        let error = if (3..=5).contains(&waited) { 11 } else { 7 };
        match unsafe { abort(device, v, id, error, if error == 7 { 1 } else { 2 }) } {
            Abort::Retry => continue,
            Abort::Done(done) => return done,
            Abort::Wait => {
                slices += 1;
                if report_slice(slices) {
                    unsafe {
                        block_diagnostics::wait_report(device, id, slices, &mut incident.0);
                    }
                }
                unsafe {
                    queue_wait(
                        v.completion_available,
                        SLICE,
                        b"block-direct-completion",
                        epoch,
                    );
                }
            }
        }
    }
}
/// # Safety
/// IRQ caller forwards the original opaque handle and physical result. Native
/// registry metadata stays admitted until the copied exact slot is validated;
/// the stateless view adapter must never wait, allocate, mutate or re-enter.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_complete(handle: u64, result: i32, bytes: u32) {
    unsafe {
        complete_receipt(handle, result, bytes);
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CompletionReceipt {
    Accepted,
    Duplicate,
    Late,
}
/// The existing IRQ completion body, with its exact disposition for the caller.
/// Normal callbacks discard this value; no diagnostic hook or global reset.
#[inline(always)]
pub(crate) unsafe fn complete_receipt(handle: u64, result: i32, bytes: u32) -> CompletionReceipt {
    let encoded_request = handle & 255;
    let encoded_device = (handle >> 8) & 255;
    if encoded_request == 0 || encoded_device == 0 {
        accounting::r4os_rust_block_runtime_change(1, 27, 1);
        return CompletionReceipt::Late;
    }
    let request_index = (encoded_request - 1) as usize;
    let device_index = (encoded_device - 1) as usize;
    if device_index >= 16 || request_index >= 16 {
        accounting::r4os_rust_block_runtime_change(1, 27, 1);
        return CompletionReceipt::Late;
    }
    let lane = {
        let _owner = Guard::acquire(1);
        let mut registered = MaybeUninit::uninit();
        if !unsafe {
            block_registry::r4os_rust_block_registry_slot(
                device_index,
                0,
                0,
                true,
                registered.as_mut_ptr(),
            )
        } {
            accounting::r4os_rust_block_runtime_change(1, 27, 1);
            return CompletionReceipt::Late;
        }
        let registered = unsafe { registered.assume_init() };
        let v = unsafe { view(registered.device) };
        let p = unsafe { slot(v, request_index) };
        if unsafe { (*p).state != 2 || (*p).backend_handle != handle } {
            stat(v, 1, 40, 1);
            accounting::r4os_rust_block_runtime_change(1, 27, 1);
            return CompletionReceipt::Late;
        }
        if !unsafe {
            dispatch::r4os_rust_storage_latch(
                ptr::addr_of_mut!((*p).completion_latch),
                1,
                handle,
                result,
                bytes,
                ptr::null_mut(),
            )
        } {
            stat(v, 1, 39, 1);
            accounting::r4os_rust_block_runtime_change(1, 26, 1);
            return CompletionReceipt::Duplicate;
        }
        block_workers::r4os_rust_block_worker_lane(v.lane)
    };
    if lane != 255 {
        accounting::r4os_rust_block_runtime_change(1, 2, 1);
        block_workers::r4os_rust_block_worker_signal(lane);
    }
    CompletionReceipt::Accepted
}
/// # Safety
/// Boot/runtime caller owns this registered index; exact operation pin/unwind
/// remains through all cancellation, real physical reset and sibling drains.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_reset(index: usize, reason: u32) -> bool {
    let mut pin = MaybeUninit::uninit();
    if !unsafe { block_registry::r4os_rust_block_registry_pin(index, pin.as_mut_ptr()) } {
        return false;
    }
    let mut pin = unsafe { pin.assume_init() };
    let done = unsafe { reset(pin.device, reason) };
    unsafe {
        block_registry::r4os_rust_block_registry_unpin(&mut pin);
    }
    done
}
unsafe fn reset(device: *mut c_void, reason: u32) -> bool {
    let v = unsafe { view(device) };
    if !v.has_reset || !unsafe { crate::storage::block_device::callbacks_allowed(device) } {
        return false;
    }
    let mut active: [MaybeUninit<block_state::Execution>; 16] =
        [const { MaybeUninit::uninit() }; 16];
    let mut cancel_needed = [false; 16];
    let mut count = 0;
    let owner = unsafe { lock(device, v) };
    if !state(v, 4, 0, 0, 0) {
        return false;
    }
    for index in 0..16 {
        let p = unsafe { slot(v, index) };
        let mut execution = MaybeUninit::uninit();
        let mut first_cancel = false;
        match unsafe {
            block_state::r4os_rust_block_slot_reset(
                p,
                index,
                execution.as_mut_ptr(),
                &mut first_cancel,
            )
        } {
            1 => {
                state(v, 10, unsafe { (*p).kind }, 12, 0);
            }
            2 => {
                if count < 16 {
                    active[count] = execution;
                    cancel_needed[count] = first_cancel;
                    count += 1;
                }
            }
            0 => {}
            _ => unreachable!(),
        }
    }
    unsafe {
        wake(v, v.completion_available, true);
    }
    drop(owner);
    for i in 0..count {
        if cancel_needed[i] {
            unsafe {
                cancel(device, v, active[i].assume_init_read(), 3);
            }
        }
    }
    stat(v, 1, 38, 1);
    accounting::r4os_rust_block_runtime_change(1, 25, 1);
    // A zero backend result is the existing independent physical quiescence
    // acknowledgement. No stale completion or logical close can substitute.
    if unsafe { crate::storage::block_device::backend(device, ptr::null(), 6, reason) } != 0 {
        let _lock = unsafe { lock(device, v) };
        state(v, 5, 0, 0, 0);
        return false;
    }
    for request in &active[..count] {
        let original = unsafe { request.assume_init_read() };
        let owner = unsafe { lock(device, v) };
        if original.slot_index >= 16 {
            continue;
        }
        let p = unsafe { slot(v, original.slot_index) };
        let matches = {
            let _owner = Guard::acquire(1);
            unsafe {
                (*p).state == 2
                    && (*p).id == original.id
                    && (*p).backend_handle == original.backend_handle
            }
        };
        if !matches {
            continue;
        }
        let mut current = MaybeUninit::uninit();
        unsafe {
            block_state::r4os_rust_block_execution(original.slot_index, p, current.as_mut_ptr());
        }
        let current = unsafe { current.assume_init() };
        drop(owner);
        unsafe {
            finish(device, v, current, false, 12);
        }
    }
    let _lock = unsafe { lock(device, v) };
    state(v, 6, 0, 0, 0);
    unsafe {
        wake(v, v.slot_available, true);
        wake(v, v.completion_available, true);
    }
    true
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Retirement {
    pub index: usize,
    pub generation: u64,
    pub active: bool,
}
unsafe fn mounted(index: usize) -> bool {
    for letter in 0..26 {
        let mut device = 0;
        if unsafe { block_mount_index(letter, &mut device) } && device == index {
            return true;
        }
    }
    false
}
unsafe fn registered(index: usize, mode: u8, generation: u64) -> Option<block_registry::SlotView> {
    let mut out = MaybeUninit::uninit();
    if unsafe {
        block_registry::r4os_rust_block_registry_slot(
            index,
            mode,
            generation,
            false,
            out.as_mut_ptr(),
        )
    } {
        Some(unsafe { out.assume_init() })
    } else {
        None
    }
}
unsafe fn retirement(index: usize, operation: u8) -> Option<block_registry::SlotView> {
    let mut out = MaybeUninit::uninit();
    if unsafe {
        block_registry::r4os_rust_block_registry_retire(index, operation, out.as_mut_ptr())
    } {
        Some(unsafe { out.assume_init() })
    } else {
        None
    }
}
unsafe fn queue_control(queue: *mut WaitQueue, operation: u8, value: u64) -> u64 {
    unsafe {
        sync_wait::r4os_rust_sync_queue(queue, operation, ptr::null_mut(), value, ptr::null_mut())
    }
}
unsafe fn reopen(v: View) {
    unsafe {
        queue_control(v.slot_available, 5, 0);
        queue_control(v.completion_available, 5, 0);
        queue_control(ptr::addr_of_mut!((*v.mutex).queue), 5, 0);
    }
}
unsafe fn quiescent(v: View) -> bool {
    let idle = {
        let _owner = Guard::acquire(1);
        unsafe {
            !v.state.resetting.read()
                && v.state.active.read() == 0
                && select(v, 3, 0) == 0
                && (*v.mutex).owner == 0
                && (*v.mutex).depth == 0
        }
    };
    if !idle {
        return false;
    }
    unsafe {
        queue_control(v.slot_available, 6, 0) == 0
            && queue_control(v.completion_available, 6, 0) == 0
            && queue_control(ptr::addr_of_mut!((*v.mutex).queue), 6, 0) == 0
    }
}
/// # Safety
/// Output is exclusive C token storage; this call owns the retirement barrier.
/// No descriptor or synchronization storage is freed before exact commit.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_retirement_prepare(
    index: usize,
    out: *mut Retirement,
) -> bool {
    unsafe { prepare_retirement(index, None, out) }
}
#[inline(always)]
unsafe fn prepare_retirement(
    index: usize,
    expected: Option<crate::storage::access_state::Reference>,
    out: *mut Retirement,
) -> bool {
    let begun = if let Some(reference) = expected {
        let mut slot = MaybeUninit::uninit();
        if unsafe { block_registry::begin_retirement(reference, slot.as_mut_ptr()) } {
            Some(unsafe { slot.assume_init() })
        } else {
            None
        }
    } else {
        unsafe { retirement(index, 0) }
    };
    let Some(slot) = begun else {
        return false;
    };
    if unsafe { mounted(index) } {
        unsafe {
            retirement(index, 1);
        }
        return false;
    }
    let v = unsafe { view(slot.device) };
    if !unsafe { quiescent(v) } {
        unsafe {
            retirement(index, 1);
        }
        return false;
    }
    let slot_waiters = unsafe { queue_control(v.slot_available, 4, 3) };
    let completion_waiters = unsafe { queue_control(v.completion_available, 4, 3) };
    let mutex_waiters = unsafe { queue_control(ptr::addr_of_mut!((*v.mutex).queue), 4, 3) };
    if slot_waiters != 0 || completion_waiters != 0 || mutex_waiters != 0 {
        unsafe {
            reopen(v);
            retirement(index, 1);
        }
        return false;
    }
    unsafe {
        out.write(Retirement {
            index,
            generation: slot.retire_generation,
            active: true,
        });
    }
    true
}
/// # Safety
/// Caller owns the exact generation-bound C retirement token through commit;
/// copied stale tokens are checked before any native registry mutation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_retirement_commit(token: *mut Retirement) -> bool {
    let a = unsafe { token.read() };
    if !a.active || unsafe { registered(a.index, 2, a.generation) }.is_none() {
        return false;
    }
    if unsafe { retirement(a.index, 2) }.is_none() {
        return false;
    }
    unsafe {
        (*token).active = false;
    }
    true
}
/// # Safety
/// Exact initialized caller token. Native admission/queues stay resident until
/// the generations are validated and original close barriers are reopened.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_retirement_cancel(token: *mut Retirement) -> bool {
    let a = unsafe { token.read() };
    if !a.active {
        return false;
    }
    let Some(slot) = (unsafe { registered(a.index, 2, a.generation) }) else {
        unsafe {
            (*token).active = false;
        }
        return false;
    };
    let v = unsafe { view(slot.device) };
    unsafe {
        reopen(v);
        retirement(a.index, 1);
        (*token).active = false;
    }
    true
}
/// # Safety
/// Exact registered index under the same prepare/commit/cancel contract. No
/// partial removal: failed commit returns all original admission barriers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_unregister(index: usize) -> bool {
    unsafe { unregister(index, None) }
}
pub(crate) unsafe fn unregister_exact(reference: crate::storage::access_state::Reference) -> bool {
    unsafe { unregister(reference.slot as usize, Some(reference)) }
}
#[inline(always)]
unsafe fn unregister(
    index: usize,
    expected: Option<crate::storage::access_state::Reference>,
) -> bool {
    let mut token = MaybeUninit::uninit();
    if !unsafe { prepare_retirement(index, expected, token.as_mut_ptr()) } {
        return false;
    }
    let mut token = unsafe { token.assume_init() };
    if unsafe { r4os_rust_block_retirement_commit(&mut token) } {
        return true;
    }
    unsafe {
        r4os_rust_block_retirement_cancel(&mut token);
    }
    false
}
/// # Safety
/// Current exact native controller worker executes this bounded scan; every
/// selected device is independently pinned before its actual queue/backend.
pub unsafe fn pump_controller(lane: u8) -> bool {
    accounting::r4os_rust_block_runtime_change(1, 5, 1);
    let mut worked = false;
    let mut index = 0;
    while index < block_registry::r4os_rust_block_registry_query(1, 0) {
        if let Some(slot) = unsafe { registered(index, 0, 0) }
            && !slot.retiring
            && block_workers::r4os_rust_block_worker_lane(slot.lane) == lane
        {
            let mut pin = MaybeUninit::uninit();
            if unsafe { block_registry::pin_existing_work(slot.reference, pin.as_mut_ptr()) } {
                let mut pin = unsafe { pin.assume_init() };
                if unsafe { pump(pin.device, 1) } {
                    worked = true;
                }
                unsafe {
                    block_registry::r4os_rust_block_registry_unpin(&mut pin);
                }
                scheduler::safe_reschedule();
            }
        }
        index += 1;
    }
    if worked {
        accounting::r4os_rust_block_runtime_change(1, 3, 1);
    }
    worked
}
const _: () = {
    assert!(core::mem::size_of::<Retirement>() == 24);
};
/// # Safety
/// Same immutable registration payload/initializer/output contract as the
/// canonical registry. Task creation occurs only after registry admission has
/// returned, never through a retained registry reference or storage guard.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_register(
    info: *const block_registry::Registration,
    payload: *const c_void,
    initialize: unsafe extern "C" fn(
        *mut c_void,
        *const c_void,
        *const crate::storage::access_state::Reference,
        u8,
    ),
    out: *mut block_registry::SlotView,
) -> bool {
    if !unsafe { block_registry::r4os_rust_block_registry_register(info, payload, initialize, out) }
    {
        return false;
    }
    let registered = unsafe { out.read() };
    if block_workers::r4os_rust_block_workers_alive()
        && !block_workers::r4os_rust_block_worker_alive(registered.lane)
        && !block_workers::r4os_rust_block_worker_start(registered.lane, false)
    {
        accounting::r4os_rust_block_runtime_change(1, 13, 1);
    }
    true
}
/// # Safety
/// Exact resident slot/sync addresses are unpublished during native registry
/// initialization. This call does not wait or inspect an external backend.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_sync_init(v: *const View) {
    let v = unsafe { v.read() };
    unsafe {
        if !v.io.has_submit {
            v.queue_depth.write(1);
        }
        ptr::write_bytes(v.slots, 0, 16);
        v.slot_available.write(WaitQueue::EMPTY);
        v.completion_available.write(WaitQueue::EMPTY);
        v.mutex.write(Mutex {
            name: Text::literal(b"block-device"),
            rank: 160,
            ..Mutex::EMPTY
        });
    }
}
/// # Safety
/// Boot-only visibility policy; mounted drive metadata is projected before
/// native storage admission. Existing active/mounted ownership is preserved.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_exclude(bus: u8) -> bool {
    if bus == 5 {
        return false;
    }
    for mounted in 0..26 {
        let mut index = 0;
        if unsafe { block_mount_index(mounted, &mut index) }
            && let Some(slot) = unsafe { registered(index, 1, 0) }
            && unsafe { view(slot.device) }.bus == bus
        {
            return false;
        }
    }
    let active = {
        let _owner = Guard::acquire(1);
        let mut active = false;
        let mut index = 0;
        while index < block_registry::r4os_rust_block_registry_query(1, 0) {
            let mut slot = MaybeUninit::uninit();
            if unsafe {
                block_registry::r4os_rust_block_registry_slot(index, 0, 0, true, slot.as_mut_ptr())
            } {
                let slot = unsafe { slot.assume_init() };
                let v = unsafe { view(slot.device) };
                if v.bus == bus && unsafe { v.state.active.read() } != 0 {
                    active = true;
                }
            }
            index += 1;
        }
        active
    };
    unsafe { block_registry::r4os_rust_block_registry_exclude(bus, active) }
}
/// # Safety
/// Explicit operation on a registered index; admitted native pin/unwind stays
/// with the exact incarnation through the snapshot/status/sense mutation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_device_operation(
    index: usize,
    operation: u8,
    opcode: u8,
    key: u8,
    asc: u8,
    ascq: u8,
) -> u32 {
    let mut pin = MaybeUninit::uninit();
    if !unsafe { block_registry::r4os_rust_block_registry_pin(index, pin.as_mut_ptr()) } {
        return 0;
    }
    let mut pin = unsafe { pin.assume_init() };
    let v = unsafe { view(pin.device) };
    let result = match operation {
        0 => unsafe { select(v, 3, 0) as u32 },
        1..=3 => {
            state(v, operation, 0, 0, 0);
            0
        }
        4 => {
            unsafe {
                accounting::r4os_rust_block_stats_sense(v.io.stats, opcode, key, asc, ascq);
            }
            0
        }
        _ => unreachable!(),
    };
    unsafe {
        block_registry::r4os_rust_block_registry_unpin(&mut pin);
    }
    result
}

unsafe fn block_incident_resolve(generation: u64) {
    let _ = crate::display::diag_screen::resolve(generation);
}

unsafe fn block_execution_view(device: *mut c_void, out: *mut View) {
    unsafe { out.write(crate::storage::block_device::execution_view(device)) }
}

unsafe fn block_mount_index(index: usize, out: *mut usize) -> bool {
    unsafe {
        let p = crate::fs::drive::r4os_rust_fs_drive_lookup(2, index);
        if p.is_null() || !(*p).has_block {
            return false;
        }
        out.write((*p).block_index);
        true
    }
}

pub(crate) unsafe fn reset_exact(
    reference: crate::storage::access_state::Reference,
    reason: u32,
) -> bool {
    let mut pin = MaybeUninit::uninit();
    if !unsafe { block_registry::pin_exact(reference, pin.as_mut_ptr()) } {
        return false;
    }
    let mut pin = unsafe { pin.assume_init() };
    let done = unsafe { reset(pin.device, reason) };
    unsafe {
        block_registry::r4os_rust_block_registry_unpin(&mut pin);
    }
    done
}
