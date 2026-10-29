//! Chunk preparation can grow/wait in the native heap. It holds no Runtime,
//! Work or BO owner; only the short publication uses the component admission.
use super::owner_table::{Empty, Page, Table};
use crate::{arch::runtime::RuntimeGuard, memory::heap};
pub(crate) fn ensure<T: Empty, G>(
    table: *mut Table<T>,
    index: usize,
    acquire: impl Fn() -> G,
) -> bool {
    if index >= u32::MAX as usize {
        return false;
    }
    loop {
        {
            let _guard = acquire();
            if !unsafe { Table::get(table, index) }.is_null() {
                return true;
            }
        }
        let (bytes, alignment) = Table::<T>::page_layout();
        let page = heap::r4os_rust_heap_alloc(bytes, alignment).cast::<Page<T>>();
        if page.is_null() {
            return false;
        }
        unsafe { Table::initialize(page) };
        let installed = {
            let _guard = acquire();
            unsafe { Table::get(table, index).is_null() && Table::append(table, page) }
        };
        if !installed {
            unsafe { heap::r4os_rust_heap_free(page.cast(), bytes) };
        }
    }
}
pub(crate) fn runtime<T: Empty>(table: *mut Table<T>, index: usize) -> bool {
    ensure(table, index, RuntimeGuard::acquire)
}
