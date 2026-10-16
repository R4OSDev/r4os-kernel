//! Resident process-owned notification namespace. Userland retains its own
//! mutex/condition-variable policy; only exact scheduler enrollment lives here.
use crate::{
    arch::runtime,
    contract as c,
    driver::treap::{Node, Tree},
    memory::heap,
    sched::{
        scheduler,
        sync_types::{Text, WaitQueue, WaitRequest},
        sync_wait,
        task::task_registry,
        task_context,
    },
};
use core::{
    cell::UnsafeCell,
    ffi::c_void,
    mem::{align_of, size_of},
    ptr,
};
#[repr(C)]
pub struct Owner {
    pub tree: Tree,
    pub closing: bool,
}
#[repr(C)]
struct Record {
    node: Node,
    handle: u64,
    sequence: u64,
    closed: bool,
    queue: WaitQueue,
}
struct Serial(UnsafeCell<u64>);
// SAFETY: serial reservation shares the existing scheduler runtime owner with
// tree access, close and waiter enrollment; no storage is borrowed across I/O.
unsafe impl Sync for Serial {}
static SERIAL: Serial = Serial(UnsafeCell::new(0));
unsafe fn find(owner: *mut Owner, handle: u64) -> *mut Record {
    unsafe { (*owner).tree.find(handle).cast() }
}
fn can_sleep() -> bool {
    let flags: u64;
    unsafe { core::arch::asm!("pushfq; pop {}",out(reg) flags,options(nomem,preserves_flags)) };
    if flags & 0x200 == 0 || runtime::r4os_rust_runtime_control(8, 0) != 0 {
        return false;
    }
    let task = scheduler::current();
    if task.is_null() {
        return false;
    }
    let core = unsafe { task_registry::core(task.cast()) };
    unsafe { (*core).preempt_disable_depth == 0 && (*core).held_lock_count == 0 }
}
struct Operation(task_context::Unwind);
impl Operation {
    fn enter() -> Option<Self> {
        if !can_sleep() {
            return None;
        }
        let flags = runtime::save_runtime();
        let token = unsafe { task_context::enter() };
        runtime::restore_runtime(flags);
        if token.admitted() {
            Some(Self(token))
        } else {
            None
        }
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        let flags = runtime::save_runtime();
        let result = unsafe { task_context::leave(self.0) };
        assert!(result, "Notification unwind identity mismatch");
        runtime::restore_runtime(flags);
    }
}
unsafe fn create(owner: *mut Owner, output: *mut u64) -> i32 {
    unsafe {
        if output.is_null() {
            return c::notification_error_invalid;
        }
        let Some(_operation) = Operation::enter() else {
            return c::notification_error_context;
        };
        let handle = {
            let _guard = runtime::RuntimeGuard::acquire();
            if (*owner).closing {
                return c::notification_error_closed;
            }
            if *SERIAL.0.get() == u64::MAX {
                return c::notification_error_exhausted;
            }
            *SERIAL.0.get() += 1;
            *SERIAL.0.get()
        };
        let memory = heap::r4os_rust_heap_alloc(size_of::<Record>(), align_of::<Record>());
        if memory.is_null() {
            return c::notification_error_memory;
        }
        let record = memory.cast::<Record>();
        record.write(Record {
            node: Node {
                key: 0,
                priority: 0,
                parent: ptr::null_mut(),
                children: [ptr::null_mut(); 2],
            },
            handle,
            sequence: 0,
            closed: false,
            queue: WaitQueue::EMPTY,
        });
        {
            let _guard = runtime::RuntimeGuard::acquire();
            assert!(!(*owner).closing);
            assert!(find(owner, handle).is_null());
            (*owner)
                .tree
                .insert(handle, ptr::addr_of_mut!((*record).node));
        }
        output.write(handle);
        c::notification_ok
    }
}
unsafe fn query(owner: *mut Owner, handle: u64, output: *mut u64) -> i32 {
    unsafe {
        if handle == 0 || output.is_null() {
            return c::notification_error_invalid;
        }
        let value = {
            let _guard = runtime::RuntimeGuard::acquire();
            let record = find(owner, handle);
            if record.is_null() {
                return c::notification_error_stale;
            }
            if (*record).closed || (*owner).closing {
                return c::notification_error_closed;
            }
            (*record).sequence
        };
        // Output can be pageable. The runtime owner is already released.
        output.write(value);
        c::notification_ok
    }
}
unsafe fn notify(owner: *mut Owner, handle: u64, count: u32) -> i32 {
    unsafe {
        if handle == 0 || count == 0 {
            return c::notification_error_invalid;
        }
        let _guard = runtime::RuntimeGuard::acquire();
        let record = find(owner, handle);
        if record.is_null() {
            return c::notification_error_stale;
        }
        if (*record).closed || (*owner).closing {
            return c::notification_error_closed;
        }
        let queue = ptr::addr_of_mut!((*record).queue);
        if (*record).sequence == u64::MAX {
            (*record).closed = true;
            sync_wait::r4os_rust_sync_queue(queue, 4, ptr::null_mut(), 3, ptr::null_mut());
            return c::notification_error_exhausted;
        }
        (*record).sequence += 1;
        let mut remaining = count;
        while remaining != 0
            && sync_wait::r4os_rust_sync_queue(queue, 0, ptr::null_mut(), 0, ptr::null_mut()) != 0
        {
            remaining -= 1;
        }
        c::notification_ok
    }
}
struct Wait {
    record: *mut Record,
    observed: u64,
}
unsafe extern "C" fn needed(raw: *mut c_void) -> bool {
    unsafe {
        let wait = raw.cast::<Wait>();
        (*(*wait).record).sequence == (*wait).observed
    }
}
unsafe fn wait(owner: *mut Owner, handle: u64, observed: u64, timeout: u64) -> i32 {
    unsafe {
        if handle == 0 {
            return c::notification_error_invalid;
        }
        if !can_sleep() {
            return c::notification_error_context;
        }
        let flags = runtime::save_runtime();
        let record = find(owner, handle);
        if record.is_null() || (*record).closed || (*owner).closing {
            let result = if record.is_null() {
                c::notification_error_stale
            } else {
                c::notification_error_closed
            };
            runtime::restore_runtime(flags);
            return result;
        }
        let mut wait = Wait { record, observed };
        let mut request = WaitRequest::predicate(
            timeout,
            Text::literal(b"program-notification"),
            needed,
            ptr::from_mut(&mut wait).cast(),
        );
        request.consume_runtime = true;
        request.flags = flags;
        // Queue admission consumes the exact outer owner. No unwind pin spans
        // this wait: close/kill can drain/free the record. Only Task is read
        // after parking; this function never dereferences `record` afterwards.
        match sync_wait::r4os_rust_sync_wait(ptr::addr_of_mut!((*record).queue), &request) {
            1 => c::notification_ok,
            2 => c::notification_timeout,
            3 | 4 => c::notification_error_closed,
            0 | 5 => c::notification_error_context,
            _ => panic!("Invalid notification wait result"),
        }
    }
}
unsafe fn remove(owner: *mut Owner, handle: u64) -> i32 {
    unsafe {
        let record = {
            let _guard = runtime::RuntimeGuard::acquire();
            let record = find(owner, handle);
            if record.is_null() {
                return c::notification_error_stale;
            }
            (*record).closed = true;
            sync_wait::r4os_rust_sync_queue(
                ptr::addr_of_mut!((*record).queue),
                4,
                ptr::null_mut(),
                3,
                ptr::null_mut(),
            );
            assert!((*record).queue.core.count == 0);
            (*owner).tree.remove(ptr::addr_of_mut!((*record).node));
            record
        };
        if heap::r4os_rust_heap_free(record.cast(), size_of::<Record>()) == 0 {
            return c::notification_ok;
        }
        let _guard = runtime::RuntimeGuard::acquire();
        assert!(find(owner, handle).is_null());
        (*owner)
            .tree
            .insert(handle, ptr::addr_of_mut!((*record).node));
        c::notification_error_release
    }
}
unsafe fn close(owner: *mut Owner, handle: u64) -> i32 {
    if handle == 0 {
        return c::notification_error_invalid;
    }
    let Some(_operation) = Operation::enter() else {
        return c::notification_error_context;
    };
    unsafe { remove(owner, handle) }
}
/// # Safety
/// Owner is the exact embedded namespace; all its tasks have retired. Failed
/// backing release retains the same closed record and prevents owner reuse.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_notifications_cleanup(owner: *mut Owner) -> bool {
    unsafe {
        {
            let _guard = runtime::RuntimeGuard::acquire();
            (*owner).closing = true;
            if (*owner).tree.root.is_null() {
                return true;
            }
        }
        let Some(_operation) = Operation::enter() else {
            return false;
        };
        loop {
            let handle = {
                let _guard = runtime::RuntimeGuard::acquire();
                (*owner).closing = true;
                let record = (*owner).tree.root.cast::<Record>();
                if record.is_null() {
                    return true;
                }
                assert!((*record).queue.core.count == 0);
                (*record).handle
            };
            if remove(owner, handle) != c::notification_ok {
                return false;
            }
        }
    }
}
/// # Safety
/// Exact process owner and optional output retain their original admission and
/// lifetime. Query/create output may be pageable and is written without owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_notifications_call(
    operation: u8,
    owner: *mut Owner,
    handle: u64,
    value: u64,
    timeout: u64,
    output: *mut u64,
) -> i32 {
    unsafe {
        match operation {
            0 => create(owner, output),
            1 => query(owner, handle, output),
            2 => notify(owner, handle, value as u32),
            3 => wait(owner, handle, value, timeout),
            4 => close(owner, handle),
            _ => panic!("Invalid private notification operation"),
        }
    }
}
const _: () = {
    assert!(size_of::<Owner>() == 24);
    assert!(align_of::<Owner>() == 8);
    assert!(core::mem::offset_of!(Owner, closing) == 16);
    assert!(core::mem::offset_of!(Record, node) == 0);
};
