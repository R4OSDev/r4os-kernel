//! Private resident workspace for bounded kernel operations that may wait.
//! The exact task's unwind token prevents kill/reap until the heap has
//! acknowledged release. No runtime/heap owner spans the operation itself.
use super::heap;
use crate::{arch::runtime::RuntimeGuard, sched::task_context};
use core::mem;

pub(crate) struct Scratch<T> {
    pointer: *mut T,
    unwind: task_context::Unwind,
}

impl<T> Scratch<T> {
    /// Allocate uninitialized storage. Initialize fields in this final location;
    /// no T is constructed, moved, or dropped by this allocation owner.
    /// T must not own resources that require destruction after scratch use.
    pub(crate) fn new() -> Option<Self> {
        if mem::size_of::<T>() == 0 {
            return None;
        }
        let unwind = {
            let _admission = RuntimeGuard::acquire();
            unsafe { task_context::enter() }
        };
        if !unwind.admitted() {
            return None;
        }
        let pointer: *mut T =
            heap::r4os_rust_heap_alloc(mem::size_of::<T>(), mem::align_of::<T>()).cast();
        if pointer.is_null() {
            let _admission = RuntimeGuard::acquire();
            assert!(unsafe { task_context::leave(unwind) });
            return None;
        }
        Some(Self { pointer, unwind })
    }

    pub(crate) fn as_mut_ptr(&self) -> *mut T {
        self.pointer
    }
}

impl<T> Drop for Scratch<T> {
    fn drop(&mut self) {
        let released =
            unsafe { heap::r4os_rust_heap_free(self.pointer.cast(), mem::size_of::<T>()) };
        // A failed release is a terminal invariant failure. Keep the exact
        // task pinned; never report normal completion or forget that token.
        assert_eq!(released, 0, "Resident scratch release failed");
        let _admission = RuntimeGuard::acquire();
        assert!(unsafe { task_context::leave(self.unwind) });
    }
}
