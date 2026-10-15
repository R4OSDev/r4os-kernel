//! R4D semaphores retain exact resident records through real waits. Closing
//! blocks new creation; existing queues receive no synthetic permit/cancel.
use crate::{
    arch::runtime::{self, RuntimeGuard},
    contract as a,
    driver::{
        semaphore_owner as m,
        treap::{Node, Tree},
    },
    memory::heap,
    sched::{
        scheduler, sync_objects,
        sync_types::{Semaphore, WaitQueue},
        task, task_context,
    },
};
use core::{
    cell::UnsafeCell,
    mem::{MaybeUninit, size_of},
    ptr,
};
const CAP: usize = crate::driver::registry::MAX_DRIVERS;
#[repr(C)]
struct State {
    owners: [m::Owner; CAP],
    next_handle: u64,
}
struct Storage(UnsafeCell<State>);
// SAFETY: the original scheduler runtime owner serializes all metadata.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    owners: [const {
        m::Owner {
            stats: m::Stats {
                epoch: 0,
                records: 0,
                private: 0,
                creates: 0,
                create_failures: 0,
                destroys: 0,
                destroy_failures: 0,
                pending_creates: 0,
                pending_destroys: 0,
                acquires: 0,
                closing: false,
            },
            tree: Tree {
                root: ptr::null_mut(),
                xorshift: 0,
            },
        }
    }; CAP],
    next_handle: 1,
}));
#[repr(C)]
struct Record {
    header: m::Record,
    payload: Semaphore,
}
const _: () = {
    assert!(size_of::<Record>() == 120);
    assert!(core::mem::offset_of!(Record, payload) == 72);
};
fn owners() -> *mut m::Owner {
    unsafe { ptr::addr_of_mut!((*STATE.0.get()).owners).cast() }
}
fn code(c: u8) -> i32 {
    match c {
        0 => a::driver_semaphore_ok,
        1 => a::driver_semaphore_error_invalid,
        2 => a::driver_semaphore_error_owner,
        3 => a::driver_semaphore_error_stale,
        4 => a::driver_semaphore_error_closed,
        5 => a::driver_semaphore_error_busy,
        6 => a::driver_semaphore_error_exhausted,
        _ => unreachable!(),
    }
}
struct Operation(task_context::Unwind);
impl Operation {
    fn enter() -> Option<Self> {
        let _g = RuntimeGuard::acquire();
        let t = unsafe { task_context::enter() };
        if t.admitted() { Some(Self(t)) } else { None }
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        let _g = RuntimeGuard::acquire();
        assert!(unsafe { task_context::leave(self.0) })
    }
}
unsafe fn valid<T>(out: *const T) -> bool {
    let head = out.cast::<u32>();
    unsafe { *head == 1 && *head.add(1) >= size_of::<T>() as u32 }
}
fn snapshot(owner: u32) -> Result<m::Stats, i32> {
    let mut value = MaybeUninit::uninit();
    let c = unsafe { m::r4os_rust_semaphore_owner_stats(owners(), CAP, owner, value.as_mut_ptr()) };
    if c == 0 {
        Ok(unsafe { value.assume_init() })
    } else {
        Err(code(c))
    }
}
fn record(owner: u32, handle: u64, op: u8) -> Result<*mut Record, i32> {
    let mut out = ptr::null_mut();
    let c =
        unsafe { m::r4os_rust_semaphore_owner_lookup(owners(), CAP, owner, handle, op, &mut out) };
    if c == 0 { Ok(out.cast()) } else { Err(code(c)) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_semaphore_can_sleep() -> bool {
    let flags: u64;
    unsafe { core::arch::asm!("pushfq","pop {}",out(reg)flags,options(preserves_flags)) };
    if flags & (1 << 9) == 0 || runtime::r4os_rust_runtime_control(8, 0) != 0 {
        return false;
    }
    let task = scheduler::current();
    if task.is_null() {
        return false;
    }
    let view = unsafe { task::scheduler_view(task) };
    view.preempt_depth == 0 && view.held_locks == 0
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_semaphore_bind(owner: u32, epoch: u64) -> bool {
    let _g = RuntimeGuard::acquire();
    unsafe { m::r4os_rust_semaphore_owner_mutate(owners(), CAP, owner, epoch, 0) == 0 }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_semaphore_discard(owner: u32, epoch: u64) -> bool {
    let _g = RuntimeGuard::acquire();
    let Ok(s) = snapshot(owner) else { return false };
    if s.epoch != epoch || s.records != 0 || s.pending_creates != 0 {
        return false;
    }
    unsafe {
        m::r4os_rust_semaphore_owner_mutate(owners(), CAP, owner, 0, 1);
        m::r4os_rust_semaphore_owner_mutate(owners(), CAP, owner, 0, 2) == 0
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_semaphore_query(owner: u32) -> i32 {
    let _g = RuntimeGuard::acquire();
    match snapshot(owner) {
        Err(c) => c,
        Ok(s) => {
            if s.closing {
                a::driver_semaphore_error_closed
            } else {
                a::driver_semaphore_ok
            }
        }
    }
}
/// # Safety
/// Private C records stay aligned and resident under the runtime owner. Output
/// headers/buffers are valid caller storage; exact wait/free tickets retain identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_semaphore_create(
    owner: u32,
    initial: u32,
    maximum: u32,
    out: *mut u64,
) -> i32 {
    unsafe { *out = 0 };
    if maximum == 0 || initial > maximum {
        return a::driver_semaphore_error_invalid;
    }
    if !r4os_rust_driver_semaphore_can_sleep() {
        return a::driver_semaphore_error_context;
    }
    let Some(_operation) = Operation::enter() else {
        return a::driver_semaphore_error_busy;
    };
    let mut ticket = MaybeUninit::uninit();
    {
        let _g = RuntimeGuard::acquire();
        let c = unsafe {
            m::r4os_rust_semaphore_owner_begin(
                owners(),
                CAP,
                ptr::addr_of_mut!((*STATE.0.get()).next_handle),
                owner,
                ticket.as_mut_ptr(),
            )
        };
        if c != 0 {
            return code(c);
        }
    }
    let mut ticket = unsafe { ticket.assume_init() };
    let memory = heap::r4os_rust_heap_alloc(size_of::<Record>(), core::mem::align_of::<Record>());
    if memory.is_null() {
        let _g = RuntimeGuard::acquire();
        assert_eq!(
            unsafe { m::r4os_rust_semaphore_owner_abort(owners(), CAP, &mut ticket) },
            0
        );
        return a::driver_semaphore_error_memory;
    }
    let record = memory.cast::<Record>();
    unsafe {
        ptr::write(
            record,
            Record {
                header: m::Record {
                    node: Node {
                        key: 0,
                        priority: 0,
                        parent: ptr::null_mut(),
                        children: [ptr::null_mut(); 2],
                    },
                    epoch: 0,
                    handle: 0,
                    owner: 0,
                    acquires: 0,
                    private: true,
                },
                payload: Semaphore {
                    count: initial,
                    max_count: maximum,
                    queue: WaitQueue::EMPTY,
                },
            },
        )
    };
    let _g = RuntimeGuard::acquire();
    let mut visible = false;
    assert_eq!(
        unsafe {
            m::r4os_rust_semaphore_owner_adopt(
                owners(),
                CAP,
                &mut ticket,
                record.cast(),
                &mut visible,
            )
        },
        0
    );
    if !visible {
        return a::driver_semaphore_error_closed;
    }
    unsafe { *out = (*record).header.handle };
    a::driver_semaphore_ok
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_semaphore_acquire(owner: u32, handle: u64, ticks: u64) -> i32 {
    if ticks == 0 {
        let _g = RuntimeGuard::acquire();
        let r = match record(owner, handle, 1) {
            Ok(r) => r,
            Err(c) => return c,
        };
        return if unsafe {
            sync_objects::r4os_rust_sync_semaphore(
                ptr::addr_of_mut!((*r).payload),
                0,
                0,
                ptr::null(),
                ptr::null_mut(),
            )
        } != 0
        {
            a::driver_semaphore_ok
        } else {
            a::driver_semaphore_error_timeout
        };
    }
    if !r4os_rust_driver_semaphore_can_sleep() {
        return a::driver_semaphore_error_context;
    }
    let Some(_operation) = Operation::enter() else {
        return a::driver_semaphore_error_busy;
    };
    let r = {
        let _g = RuntimeGuard::acquire();
        match record(owner, handle, 2) {
            Ok(r) => r,
            Err(c) => return c,
        }
    };
    let result = unsafe {
        sync_objects::r4os_rust_sync_semaphore(
            ptr::addr_of_mut!((*r).payload),
            1,
            ticks,
            ptr::null(),
            ptr::null_mut(),
        )
    };
    {
        let _g = RuntimeGuard::acquire();
        assert_eq!(
            unsafe { m::r4os_rust_semaphore_owner_acquired(owners(), CAP, r.cast()) },
            0
        )
    }
    match result {
        1 => a::driver_semaphore_ok,
        2 => a::driver_semaphore_error_timeout,
        3 | 4 => a::driver_semaphore_error_cancelled,
        0 | 5 => a::driver_semaphore_error_context,
        _ => unreachable!(),
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_semaphore_release(owner: u32, handle: u64) -> i32 {
    let _g = RuntimeGuard::acquire();
    let r = match record(owner, handle, 1) {
        Ok(r) => r,
        Err(c) => return c,
    };
    if unsafe {
        sync_objects::r4os_rust_sync_semaphore(
            ptr::addr_of_mut!((*r).payload),
            4,
            1,
            ptr::null(),
            ptr::null_mut(),
        )
    } == 1
    {
        a::driver_semaphore_ok
    } else {
        a::driver_semaphore_error_overflow
    }
}
fn remove(owner: u32, handle: u64, private: bool) -> i32 {
    let mut ticket = MaybeUninit::uninit();
    {
        let _g = RuntimeGuard::acquire();
        if !private && let Err(c) = record(owner, handle, 1) {
            return c;
        }
        let c = unsafe {
            m::r4os_rust_semaphore_owner_destroy(owners(), CAP, owner, handle, ticket.as_mut_ptr())
        };
        if c != 0 {
            return code(c);
        }
        let t = unsafe { ticket.assume_init_ref() };
        assert_eq!(
            unsafe { (*t.record.cast::<Record>()).payload.queue.core.count },
            0
        );
    }
    let mut ticket = unsafe { ticket.assume_init() };
    let success =
        unsafe { heap::r4os_rust_heap_free(ticket.record.cast(), size_of::<Record>()) } == 0;
    let _g = RuntimeGuard::acquire();
    assert_eq!(
        unsafe { m::r4os_rust_semaphore_owner_finish_destroy(owners(), CAP, &mut ticket, success) },
        0
    );
    if success {
        a::driver_semaphore_ok
    } else {
        a::driver_semaphore_error_release
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_semaphore_destroy(owner: u32, handle: u64) -> i32 {
    if !r4os_rust_driver_semaphore_can_sleep() {
        return a::driver_semaphore_error_context;
    }
    let Some(_operation) = Operation::enter() else {
        return a::driver_semaphore_error_busy;
    };
    remove(owner, handle, false)
}
/// # Safety
/// Private C records stay aligned and resident under the runtime owner. Output
/// headers/buffers are valid caller storage; exact wait/free tickets retain identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_semaphore_status(
    owner: u32,
    handle: u64,
    out: *mut a::DriverSemaphoreStatus,
) -> i32 {
    if !unsafe { valid(out) } {
        return a::driver_semaphore_error_invalid;
    }
    unsafe { ptr::write(out, a::DriverSemaphoreStatus::default()) };
    let _g = RuntimeGuard::acquire();
    let r = match record(owner, handle, 1) {
        Ok(r) => r,
        Err(c) => return c,
    };
    unsafe {
        ptr::write(
            out,
            a::DriverSemaphoreStatus {
                handle,
                owner_epoch: (*r).header.epoch,
                available: (*r).payload.count,
                maximum: (*r).payload.max_count,
                queued_waiters: (*r).payload.queue.core.count as u32,
                active_acquires: (*r).header.acquires,
                ..a::DriverSemaphoreStatus::default()
            },
        )
    };
    a::driver_semaphore_ok
}
/// # Safety
/// Private C records stay aligned and resident under the runtime owner. Output
/// headers/buffers are valid caller storage; exact wait/free tickets retain identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_semaphore_stats(
    owner: u32,
    out: *mut a::DriverSemaphoreStats,
) -> i32 {
    if !unsafe { valid(out) } {
        return a::driver_semaphore_error_invalid;
    }
    unsafe { ptr::write(out, a::DriverSemaphoreStats::default()) };
    let _g = RuntimeGuard::acquire();
    let s = match snapshot(owner) {
        Ok(s) => s,
        Err(c) => return c,
    };
    unsafe {
        ptr::write(
            out,
            a::DriverSemaphoreStats {
                owner_epoch: s.epoch,
                records: s.records,
                private_records: s.private,
                creates: s.creates,
                create_failures: s.create_failures,
                destroys: s.destroys,
                destroy_failures: s.destroy_failures,
                pending_creates: s.pending_creates,
                pending_destroys: s.pending_destroys,
                active_acquires: s.acquires,
                closing: u32::from(s.closing),
                ..a::DriverSemaphoreStats::default()
            },
        )
    };
    a::driver_semaphore_ok
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_semaphore_close(owner: u32) {
    let _g = RuntimeGuard::acquire();
    unsafe { m::r4os_rust_semaphore_owner_mutate(owners(), CAP, owner, 0, 1) };
}
#[repr(C)]
pub struct Cleanup {
    pub released: u64,
    pub quiesced: bool,
}
fn cleanup(owner: u32) -> Cleanup {
    let mut result = Cleanup {
        released: 0,
        quiesced: true,
    };
    let count = {
        let _g = RuntimeGuard::acquire();
        let s = match snapshot(owner) {
            Ok(s) => s,
            Err(c) => {
                result.quiesced = c == a::driver_semaphore_error_stale;
                return result;
            }
        };
        if !s.closing || s.pending_creates != 0 || s.pending_destroys != 0 || s.acquires != 0 {
            result.quiesced = false;
            return result;
        }
        if s.records == 0 {
            assert_eq!(
                unsafe { m::r4os_rust_semaphore_owner_mutate(owners(), CAP, owner, 0, 2) },
                0
            );
            return result;
        }
        s.records
    };
    if !r4os_rust_driver_semaphore_can_sleep() {
        result.quiesced = false;
        return result;
    }
    let Some(_operation) = Operation::enter() else {
        result.quiesced = false;
        return result;
    };
    while result.released < count {
        let handle = {
            let _g = RuntimeGuard::acquire();
            let mut r = ptr::null_mut();
            assert_eq!(
                unsafe { m::r4os_rust_semaphore_owner_first(owners(), CAP, owner, &mut r) },
                0
            );
            if r.is_null() {
                break;
            }
            unsafe { (*r).handle }
        };
        if remove(owner, handle, true) != a::driver_semaphore_ok {
            result.quiesced = false;
            return result;
        }
        result.released += 1;
    }
    let _g = RuntimeGuard::acquire();
    if unsafe { m::r4os_rust_semaphore_owner_mutate(owners(), CAP, owner, 0, 2) } != 0 {
        result.quiesced = false
    }
    result
}
/// # Safety
/// Private C records stay aligned and resident under the runtime owner. Output
/// headers/buffers are valid caller storage; exact wait/free tickets retain identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_semaphore_cleanup(owner: u32, out: *mut Cleanup) {
    unsafe { ptr::write(out, cleanup(owner)) };
}
