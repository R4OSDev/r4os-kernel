//! Process-local publication associations. Userland retains its data and
//! allocator; the kernel owns only resident intrusive association records.
use crate::{
    arch::runtime::{self, RuntimeGuard},
    contract as a,
    driver::treap::{Node, Tree},
    memory::heap,
    sched::{scheduler, task::task_registry, task_context},
};
use core::{mem, ptr};

#[repr(C)]
struct Record {
    node: Node,
    retired_next: *mut Record,
    _key: u64,
    value: u64,
}

#[repr(C)]
#[derive(Default)]
pub struct Owner {
    tree: Tree,
    retired: *mut Record,
    closing: bool,
}

struct Operation(task_context::Unwind);
impl Operation {
    fn enter() -> Option<Self> {
        let flags: u64;
        unsafe { core::arch::asm!("pushfq", "pop {}", out(reg) flags, options(preserves_flags)) };
        if flags & (1 << 9) == 0 || runtime::r4os_rust_runtime_control(8, 0) != 0 {
            return None;
        }
        let task = scheduler::current();
        if task.is_null() {
            return None;
        }
        // These are the original current-task-only pre-admission checks.
        let core = unsafe { task_registry::core(task.cast()) };
        if unsafe { (*core).preempt_disable_depth != 0 || (*core).held_lock_count != 0 } {
            return None;
        }
        let _guard = RuntimeGuard::acquire();
        let token = unsafe { task_context::enter() };
        if token.admitted() {
            Some(Self(token))
        } else {
            None
        }
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        let _guard = RuntimeGuard::acquire();
        assert!(unsafe { task_context::leave(self.0) });
    }
}

unsafe fn find(owner: *mut Owner, key: u64) -> *mut Record {
    unsafe { (*owner).tree.find(key).cast() }
}

unsafe fn free(record: *mut Record) -> bool {
    unsafe { heap::r4os_rust_heap_free(record.cast(), mem::size_of::<Record>()) == 0 }
}

unsafe fn retain(owner: *mut Owner, record: *mut Record) {
    let _guard = RuntimeGuard::acquire();
    unsafe {
        (*record).retired_next = (*owner).retired;
        (*owner).retired = record;
    }
}

/// # Safety
/// Owner is resident through the call; a non-null output names writable caller
/// storage. No owner reference spans the potentially faulting output write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_local_get(
    owner: *mut Owner,
    key: u64,
    output: *mut u64,
) -> i32 {
    if key == 0 || output.is_null() {
        return a::program_local_error_invalid;
    }
    let value = {
        let _guard = RuntimeGuard::acquire();
        if unsafe { (*owner).closing } {
            return a::program_local_error_closed;
        }
        let record = unsafe { find(owner, key) };
        if record.is_null() {
            0
        } else {
            unsafe { (*record).value }
        }
    };
    unsafe { output.write(value) };
    a::program_local_ok
}

/// # Safety
/// Owner and writable output retain the exact process lifetime. The admitted
/// unwind token protects association publication/disposal through allocation,
/// possible waits and the final caller-owned output write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_local_publish(
    owner: *mut Owner,
    key: u64,
    candidate_value: u64,
    output: *mut u64,
) -> i32 {
    if key == 0 || candidate_value == 0 || output.is_null() {
        return a::program_local_error_invalid;
    }
    let Some(_operation) = Operation::enter() else {
        return a::program_local_error_context;
    };
    let previous = {
        let _guard = RuntimeGuard::acquire();
        if unsafe { (*owner).closing } {
            return a::program_local_error_closed;
        }
        let record = unsafe { find(owner, key) };
        if record.is_null() {
            0
        } else {
            unsafe { (*record).value }
        }
    };
    if previous != 0 {
        unsafe { output.write(previous) };
        return a::program_local_existing;
    }
    let candidate = heap::r4os_rust_heap_alloc(mem::size_of::<Record>(), mem::align_of::<Record>())
        .cast::<Record>();
    if candidate.is_null() {
        return a::program_local_error_memory;
    }
    // The tree initializes its node only on insertion. No reference to this
    // still-uninitialized node is formed before that exact initialization.
    unsafe {
        ptr::addr_of_mut!((*candidate).retired_next).write(ptr::null_mut());
        ptr::addr_of_mut!((*candidate)._key).write(key);
        ptr::addr_of_mut!((*candidate).value).write(candidate_value);
    }
    let winner = {
        let _guard = RuntimeGuard::acquire();
        assert!(!unsafe { (*owner).closing });
        let existing = unsafe { find(owner, key) };
        if existing.is_null() {
            unsafe { (*owner).tree.insert(key, candidate.cast()) };
            candidate
        } else {
            existing
        }
    };
    let value = unsafe { (*winner).value };
    if winner != candidate && !unsafe { free(candidate) } {
        unsafe { retain(owner, candidate) };
    }
    unsafe { output.write(value) };
    if winner == candidate {
        a::program_local_ok
    } else {
        a::program_local_existing
    }
}

/// # Safety
/// The process reaper retains this exact owner through confirmed association
/// disposal. User values remain untouched until their later process VM teardown.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_local_cleanup(owner: *mut Owner) -> bool {
    {
        let _guard = RuntimeGuard::acquire();
        unsafe { (*owner).closing = true };
        if unsafe { (*owner).tree.root.is_null() && (*owner).retired.is_null() } {
            return true;
        }
    }
    let Some(_operation) = Operation::enter() else {
        return false;
    };
    loop {
        let record = {
            let _guard = RuntimeGuard::acquire();
            let retired = unsafe { (*owner).retired };
            if !retired.is_null() {
                unsafe {
                    (*owner).retired = (*retired).retired_next;
                    (*retired).retired_next = ptr::null_mut();
                }
                retired
            } else {
                let node = unsafe { (*owner).tree.root };
                if node.is_null() {
                    return true;
                }
                unsafe { (*owner).tree.remove(node) };
                node.cast()
            }
        };
        if !unsafe { free(record) } {
            unsafe { retain(owner, record) };
            return false;
        }
    }
}

const _: () = {
    assert!(mem::size_of::<Owner>() == 32);
    assert!(mem::align_of::<Owner>() == 8);
    assert!(mem::offset_of!(Owner, retired) == 16);
    assert!(mem::offset_of!(Owner, closing) == 24);
    assert!(mem::size_of::<Record>() == 64);
    assert!(mem::offset_of!(Record, node) == 0);
    assert!(mem::offset_of!(Record, retired_next) == 40);
    assert!(mem::offset_of!(Record, _key) == 48);
    assert!(mem::offset_of!(Record, value) == 56);
};
