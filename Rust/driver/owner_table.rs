//! Resident per-owner rows. The inline BSP allocation is bootstrap storage,
//! not an admission limit. Published chunks never move or retire while the
//! kernel lives; each component keeps its original row/epoch admission.
use core::{
    cell::UnsafeCell,
    mem, ptr,
    sync::atomic::{AtomicPtr, AtomicUsize, Ordering},
};
pub(crate) const CHUNK: usize = 32;
pub(crate) trait Empty {
    const EMPTY: Self;
}
#[repr(C)]
pub(crate) struct Page<T> {
    next: AtomicPtr<Page<T>>,
    rows: UnsafeCell<[T; CHUNK]>,
}
pub(crate) struct Table<T> {
    inline: UnsafeCell<[T; CHUNK]>,
    head: AtomicPtr<Page<T>>,
    tail: UnsafeCell<*mut Page<T>>,
    capacity: AtomicUsize,
}
impl<T: Empty> Table<T> {
    pub(crate) const fn new() -> Self {
        Self {
            inline: UnsafeCell::new([const { T::EMPTY }; CHUNK]),
            head: AtomicPtr::new(ptr::null_mut()),
            tail: UnsafeCell::new(ptr::null_mut()),
            capacity: AtomicUsize::new(CHUNK),
        }
    }
    /// Initialize unlinked exclusive allocation; no large temporary on a task stack.
    pub(crate) unsafe fn initialize(page: *mut Page<T>) {
        unsafe {
            ptr::addr_of_mut!((*page).next).write(AtomicPtr::new(ptr::null_mut()));
            for i in 0..CHUNK {
                ptr::addr_of_mut!((*page).rows)
                    .cast::<T>()
                    .add(i)
                    .write(T::EMPTY);
            }
        }
    }
}
impl<T> Table<T> {
    pub(crate) fn capacity(&self) -> usize {
        self.capacity.load(Ordering::Acquire)
    }
    /// Caller protects mutable row contents. Append publication alone is atomic.
    pub(crate) unsafe fn get(table: *mut Self, index: usize) -> *mut T {
        unsafe {
            if index < CHUNK {
                return (*table).inline.get().cast::<T>().add(index);
            }
            if index >= (*table).capacity() {
                return ptr::null_mut();
            }
            let mut page = (*table).head.load(Ordering::Acquire);
            let mut block = index / CHUNK - 1;
            while !page.is_null() && block != 0 {
                page = (*page).next.load(Ordering::Acquire);
                block -= 1;
            }
            if page.is_null() {
                return ptr::null_mut();
            }
            ptr::addr_of_mut!((*page).rows)
                .cast::<T>()
                .add(index % CHUNK)
        }
    }
    /// Component writer admission held; page is fully initialized and unlinked.
    pub(crate) unsafe fn append(table: *mut Self, page: *mut Page<T>) -> bool {
        unsafe {
            let Some(capacity) = (*table).capacity().checked_add(CHUNK) else {
                return false;
            };
            let tail = (*table).tail.get().read();
            if tail.is_null() {
                (*table).head.store(page, Ordering::Release);
            } else {
                (*tail).next.store(page, Ordering::Release);
            }
            (*table).tail.get().write(page);
            (*table).capacity.store(capacity, Ordering::Release);
            true
        }
    }
    pub(crate) const fn page_layout() -> (usize, usize) {
        (mem::size_of::<Page<T>>(), mem::align_of::<Page<T>>())
    }
}
/// Private resident model transport. It keeps the full global owner identity;
/// no chunk-local rebasing or movable borrowed array crosses a model call.
#[repr(C)]
pub struct Rows<T> {
    table: *mut Table<T>,
}
impl<T> Copy for Rows<T> {}
impl<T> Clone for Rows<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Rows<T> {
    pub(crate) const fn resident(table: *mut Table<T>) -> Self {
        Self { table }
    }
    pub(crate) unsafe fn len(self) -> usize {
        unsafe { (*self.table).capacity() }
    }
    pub(crate) unsafe fn get(self, index: usize) -> *mut T {
        unsafe { Table::get(self.table, index) }
    }
}
