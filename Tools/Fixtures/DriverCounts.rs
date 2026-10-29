//! Composition checks for actual owner tables/storage and resource algorithms.
//! Runtime/heap stand-ins exercise lock exclusion and injected allocation refusal;
//! they do not claim real tasks, DMA hardware or a guest boot.
use crate::driver::{
    memory_owner as h, owner_storage,
    owner_table::{Empty, Rows, Table},
    resource_state as r, semaphore_owner as s, thread_owner as t,
};
pub use std::sync::atomic::Ordering;
use std::{
    mem,
    sync::{Mutex, atomic::AtomicBool},
};
thread_local! {pub static DEPTH:std::cell::Cell<usize>=const{std::cell::Cell::new(0)};}
pub static FAIL: AtomicBool = AtomicBool::new(false);
pub static ALLOCS: Mutex<Vec<(usize, usize, usize)>> = Mutex::new(Vec::new());
pub mod arch {
    pub mod runtime {
        pub struct RuntimeGuard;
        impl RuntimeGuard {
            pub fn acquire() -> Self {
                crate::DEPTH.with(|v| v.set(v.get() + 1));
                Self
            }
        }
        impl Drop for RuntimeGuard {
            fn drop(&mut self) {
                crate::DEPTH.with(|v| v.set(v.get() - 1));
            }
        }
    }
}
pub mod memory {
    pub mod heap {
        pub fn r4os_rust_heap_alloc(bytes: usize, align: usize) -> *mut u8 {
            crate::DEPTH.with(|v| assert_eq!(v.get(), 0, "allocation under metadata owner"));
            if crate::FAIL.load(crate::Ordering::Relaxed) {
                return core::ptr::null_mut();
            }
            let p = unsafe {
                std::alloc::alloc(std::alloc::Layout::from_size_align(bytes, align).unwrap())
            };
            if !p.is_null() {
                crate::ALLOCS
                    .lock()
                    .unwrap()
                    .push((p as usize, bytes, align));
            }
            p
        }
        pub unsafe fn r4os_rust_heap_free(p: *mut u8, bytes: usize) -> u8 {
            crate::DEPTH.with(|v| assert_eq!(v.get(), 0, "free under metadata owner"));
            let mut all = crate::ALLOCS.lock().unwrap();
            let i = all.iter().position(|x| x.0 == p as usize).unwrap();
            let (_, n, a) = all.swap_remove(i);
            assert_eq!(bytes, n);
            unsafe { std::alloc::dealloc(p, std::alloc::Layout::from_size_align(n, a).unwrap()) };
            0
        }
    }
}
impl Empty for h::Owner {
    const EMPTY: Self = unsafe { mem::zeroed() };
}
impl Empty for t::Owner {
    const EMPTY: Self = unsafe { mem::zeroed() };
}
impl Empty for s::Owner {
    const EMPTY: Self = unsafe { mem::zeroed() };
}
impl Empty for r::Binding {
    const EMPTY: Self = unsafe { mem::zeroed() };
}
fn cleanup() {
    for (p, n, a) in ALLOCS.lock().unwrap().drain(..) {
        unsafe {
            std::alloc::dealloc(
                p as *mut u8,
                std::alloc::Layout::from_size_align(n, a).unwrap(),
            )
        }
    }
}
#[test]
fn actual_heap_tokens_keep_full_owner_and_live_backing_across_growth() {
    let mut table = Table::<h::Owner>::new();
    let p = &mut table as *mut _;
    assert!(owner_storage::runtime(p, 64));
    let rows = Rows::resident(p);
    unsafe {
        assert_eq!(h::r4os_rust_driver_memory_mutate(rows, 0, 65, 777), 0);
        let saved = Table::get(p, 64);
        let mut layout = mem::zeroed();
        assert_eq!(h::r4os_rust_driver_memory_layout(137, 64, &mut layout), 0);
        let mut next = 1;
        let mut c = mem::zeroed();
        assert_eq!(
            h::r4os_rust_driver_memory_begin(rows, &mut next, 65, &layout, &mut c),
            0
        );
        assert_eq!(c.owner, 65);
        assert!(owner_storage::runtime(p, 160));
        assert_eq!(Table::get(p, 64), saved);
        let l = std::alloc::Layout::from_size_align(layout.heap_bytes, layout.alignment as usize)
            .unwrap();
        let backing = std::alloc::alloc(l);
        assert!(!backing.is_null());
        let mut published = mem::zeroed();
        assert_eq!(
            h::r4os_rust_driver_memory_adopt(
                rows,
                &mut c,
                backing,
                layout.heap_bytes,
                &mut published
            ),
            0
        );
        assert!(published.visible);
        let a = published.allocation;
        assert_eq!(a.address, backing as usize + layout.payload_offset);
        (a.address as *mut u8).write_bytes(0x29, 137);
        let mut release = mem::zeroed();
        assert_ne!(
            h::r4os_rust_driver_memory_release(rows, 33, a.handle, &mut release),
            0
        );
        assert_eq!(
            h::r4os_rust_driver_memory_release(rows, 65, a.handle, &mut release),
            0
        );
        assert_eq!(release.owner, 65);
        std::alloc::dealloc(backing, l);
        assert_eq!(
            h::r4os_rust_driver_memory_finish_release(rows, &mut release, true),
            0
        );
        assert_eq!(h::r4os_rust_driver_memory_mutate(rows, 1, 65, 0), 0);
        assert_eq!(h::r4os_rust_driver_memory_mutate(rows, 2, 65, 0), 0);
    }
    cleanup();
}
#[test]
fn pending_threads_and_semaphores_keep_original_ids_not_chunk_local_ids() {
    let mut thread = Table::<t::Owner>::new();
    let mut sem = Table::<s::Owner>::new();
    assert!(owner_storage::runtime(&mut thread, 96));
    assert!(owner_storage::runtime(&mut sem, 96));
    let tr = Rows::resident(&mut thread);
    let sr = Rows::resident(&mut sem);
    unsafe {
        assert_eq!(t::r4os_rust_thread_owner_mutate(tr, 97, 88, 0), 0);
        assert_eq!(s::r4os_rust_semaphore_owner_mutate(sr, 97, 99, 0), 0);
        let (mut next, mut tc, mut sc) = (1, mem::zeroed(), mem::zeroed());
        assert_eq!(
            t::r4os_rust_thread_owner_begin(tr, &mut next, 97, &mut tc),
            0
        );
        assert_eq!(
            s::r4os_rust_semaphore_owner_begin(sr, &mut next, 97, &mut sc),
            0
        );
        assert_eq!((tc.owner, sc.owner), (97, 97));
        assert!(owner_storage::runtime(&mut thread, 192));
        assert!(owner_storage::runtime(&mut sem, 192));
        assert_eq!(t::r4os_rust_thread_owner_abort(tr, &mut tc), 0);
        assert_eq!(s::r4os_rust_semaphore_owner_abort(sr, &mut sc), 0);
    }
    cleanup();
}
#[test]
fn resource_generation_reuse_and_cross_owner_handles_survive_table_growth() {
    let mut table = Table::<r::Binding>::new();
    assert!(owner_storage::runtime(&mut table, 64));
    let rows = Rows::resident(&mut table);
    let mut state = r::State {
        next_epoch: 1,
        bindings: rows,
    };
    unsafe {
        assert_eq!(
            r::r4os_rust_resource_state_mutate(&mut state, 0, 65, 111, 7),
            0
        );
        let saved = Table::get(&mut table, 64);
        let mut handle = 0;
        assert_eq!(
            r::r4os_rust_resource_handle(&state, 0, 65, 3, &mut handle),
            0
        );
        assert!(owner_storage::runtime(&mut table, 160));
        assert_eq!(Table::get(&mut table, 64), saved);
        assert_eq!((*saved).slot, 111);
        assert_eq!(
            r::r4os_rust_resource_state_mutate(&mut state, 0, 33, 222, 8),
            0
        );
        let mut out = 0;
        assert_ne!(
            r::r4os_rust_resource_handle(&state, 1, 33, handle, &mut out),
            0
        );
        assert_eq!(
            r::r4os_rust_resource_state_mutate(&mut state, 2, 65, 0, 0),
            0
        );
        assert_eq!(
            r::r4os_rust_resource_state_mutate(&mut state, 0, 65, 333, 9),
            0
        );
        assert_ne!(
            r::r4os_rust_resource_handle(&state, 1, 65, handle, &mut out),
            0
        );
        assert_eq!((*saved).module_generation, 9);
    }
    cleanup();
}
#[test]
fn allocation_refusal_publishes_no_partial_row_or_owner_change() {
    let mut table = Table::<r::Binding>::new();
    unsafe { (*Table::get(&mut table, 0)).epoch = 42 };
    let before = table.capacity();
    let old = unsafe { Table::get(&mut table, 0) };
    FAIL.store(true, Ordering::Relaxed);
    assert!(!owner_storage::runtime(&mut table, 32));
    FAIL.store(false, Ordering::Relaxed);
    assert_eq!(table.capacity(), before);
    assert!(unsafe { Table::get(&mut table, 32) }.is_null());
    assert_eq!(unsafe { (*old).epoch }, 42);
    assert!(owner_storage::runtime(&mut table, 32));
    assert_eq!(unsafe { Table::get(&mut table, 0) }, old);
    assert_eq!(unsafe { (*old).epoch }, 42);
    cleanup();
}
