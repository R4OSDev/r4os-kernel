//! Canonical R4D CPU allocation owner. Runtime admission protects metadata;
//! actual heap I/O has only an exact task unwind token, never a metadata lock.
use crate::driver::{
    owner_storage,
    owner_table::{Empty, Rows, Table},
};
use crate::{
    arch::runtime::RuntimeGuard,
    contract as a,
    driver::{memory_owner as m, treap::Tree},
    memory::heap,
    sched::task_context,
};
use core::{
    cell::UnsafeCell,
    mem::{MaybeUninit, size_of},
    ptr,
};
#[repr(C)]
struct State {
    owners: Table<m::Owner>,
    next_handle: u64,
}
struct Storage(UnsafeCell<State>);
// SAFETY: all metadata mutation uses the existing scheduler runtime owner.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    owners: Table::new(),
    next_handle: 1,
}));
fn owner_table() -> *mut Table<m::Owner> {
    unsafe { ptr::addr_of_mut!((*STATE.0.get()).owners) }
}
fn owners() -> Rows<m::Owner> {
    Rows::resident(owner_table())
}
pub(crate) fn prepare(owner: u32) -> bool {
    owner != 0 && owner_storage::runtime(owner_table(), owner as usize - 1)
}
fn code(value: u8) -> i32 {
    match value {
        0 => a::driver_heap_ok,
        1 => a::driver_heap_error_invalid,
        2 => a::driver_heap_error_owner,
        3 => a::driver_heap_error_stale,
        4 => a::driver_heap_error_closed,
        5 => a::driver_heap_error_busy,
        6 => a::driver_heap_error_exhausted,
        7 => a::driver_heap_error_overflow,
        _ => unreachable!(),
    }
}
struct Operation(task_context::Unwind);
impl Operation {
    fn enter() -> Option<Self> {
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
        assert!(unsafe { task_context::leave(self.0) })
    }
}
unsafe fn valid<T>(output: *const T) -> bool {
    let head = output.cast::<u32>();
    unsafe { *head == 1 && *head.add(1) >= size_of::<T>() as u32 }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_heap_bind(owner: u32, epoch: u64) -> bool {
    if !prepare(owner) {
        return false;
    }
    let _g = RuntimeGuard::acquire();
    unsafe { m::r4os_rust_driver_memory_mutate(owners(), 0, owner, epoch) == 0 }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_heap_close(owner: u32) {
    let _g = RuntimeGuard::acquire();
    unsafe { m::r4os_rust_driver_memory_mutate(owners(), 1, owner, 0) };
}
fn snapshot(owner: u32) -> Result<m::Stats, i32> {
    let _g = RuntimeGuard::acquire();
    let mut s = MaybeUninit::uninit();
    let c = unsafe { m::r4os_rust_driver_memory_snapshot(owners(), owner, s.as_mut_ptr()) };
    if c != 0 {
        return Err(code(c));
    }
    Ok(unsafe { s.assume_init() })
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_heap_query(owner: u32) -> i32 {
    match snapshot(owner) {
        Err(c) => c,
        Ok(s) => {
            if s.closing {
                a::driver_heap_error_closed
            } else {
                a::driver_heap_ok
            }
        }
    }
}
/// # Safety
/// Aligned private C records are resident and exclusive under the enclosing
/// driver owner. Backend callbacks preserve that owner and exact record lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_heap_allocate(
    owner: u32,
    bytes: u64,
    alignment: u32,
    out: *mut a::DriverHeapAllocation,
) -> i32 {
    if !unsafe { valid(out) } {
        return a::driver_heap_error_invalid;
    }
    unsafe { ptr::write(out, a::DriverHeapAllocation::default()) };
    let mut layout = MaybeUninit::uninit();
    let c = unsafe { m::r4os_rust_driver_memory_layout(bytes, alignment, layout.as_mut_ptr()) };
    if c != 0 {
        return code(c);
    }
    let layout = unsafe { layout.assume_init() };
    let Some(_operation) = Operation::enter() else {
        return a::driver_heap_error_busy;
    };
    let mut create = MaybeUninit::uninit();
    {
        let _g = RuntimeGuard::acquire();
        let c = unsafe {
            m::r4os_rust_driver_memory_begin(
                owners(),
                ptr::addr_of_mut!((*STATE.0.get()).next_handle),
                owner,
                &layout,
                create.as_mut_ptr(),
            )
        };
        if c != 0 {
            return code(c);
        }
    }
    let mut create = unsafe { create.assume_init() };
    let backing = heap::r4os_rust_heap_alloc(layout.heap_bytes, layout.alignment as usize);
    if backing.is_null() {
        let _g = RuntimeGuard::acquire();
        assert_eq!(
            unsafe { m::r4os_rust_driver_memory_abort(owners(), &mut create) },
            0
        );
        return a::driver_heap_error_memory;
    }
    let mut publication = MaybeUninit::uninit();
    {
        let _g = RuntimeGuard::acquire();
        assert_eq!(
            unsafe {
                m::r4os_rust_driver_memory_adopt(
                    owners(),
                    &mut create,
                    backing,
                    layout.heap_bytes,
                    publication.as_mut_ptr(),
                )
            },
            0
        );
    }
    let p = unsafe { publication.assume_init() };
    if !p.visible {
        return a::driver_heap_error_closed;
    }
    unsafe {
        ptr::write(
            out,
            a::DriverHeapAllocation {
                handle: p.allocation.handle,
                cpu_address: p.allocation.address as u64,
                byte_length: p.allocation.bytes,
                alignment: p.allocation.alignment,
                ..a::DriverHeapAllocation::default()
            },
        )
    };
    a::driver_heap_ok
}
fn release_backing(ticket: &mut m::Release) -> i32 {
    let released =
        unsafe { heap::r4os_rust_heap_free(ticket.record.cast(), ticket.heap_bytes) } == 0;
    let _g = RuntimeGuard::acquire();
    assert_eq!(
        unsafe { m::r4os_rust_driver_memory_finish_release(owners(), ticket, released) },
        0
    );
    if released {
        a::driver_heap_ok
    } else {
        a::driver_heap_error_release
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_heap_release(owner: u32, handle: u64) -> i32 {
    let Some(_operation) = Operation::enter() else {
        return a::driver_heap_error_busy;
    };
    let mut ticket = MaybeUninit::uninit();
    {
        let _g = RuntimeGuard::acquire();
        let c = unsafe {
            m::r4os_rust_driver_memory_release(owners(), owner, handle, ticket.as_mut_ptr())
        };
        if c != 0 {
            return code(c);
        }
    }
    release_backing(&mut unsafe { ticket.assume_init() })
}
/// # Safety
/// Aligned private C records are resident and exclusive under the enclosing
/// driver owner. Backend callbacks preserve that owner and exact record lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_heap_stats(
    owner: u32,
    out: *mut a::DriverHeapStats,
) -> i32 {
    if !unsafe { valid(out) } {
        return a::driver_heap_error_invalid;
    }
    unsafe { ptr::write(out, a::DriverHeapStats::default()) };
    let s = match snapshot(owner) {
        Ok(s) => s,
        Err(c) => return c,
    };
    unsafe {
        ptr::write(
            out,
            a::DriverHeapStats {
                owner_epoch: s.epoch,
                allocations: s.allocations,
                bytes: s.bytes,
                backing_bytes: s.backing_bytes,
                peak_bytes: s.peak_bytes,
                private_allocations: s.private_allocations,
                allocation_calls: s.allocation_calls,
                allocation_failures: s.allocation_failures,
                releases: s.releases,
                release_failures: s.release_failures,
                pending_creates: s.pending_creates,
                pending_releases: s.pending_releases,
                closing: u32::from(s.closing),
                ..a::DriverHeapStats::default()
            },
        )
    };
    a::driver_heap_ok
}
#[repr(C)]
pub struct Cleanup {
    pub released: u64,
    pub bytes: u64,
    pub quiesced: bool,
}
fn cleanup(owner: u32) -> Cleanup {
    let mut result = Cleanup {
        released: 0,
        bytes: 0,
        quiesced: true,
    };
    let Some(_operation) = Operation::enter() else {
        result.quiesced = false;
        return result;
    };
    let s = match snapshot(owner) {
        Ok(s) => s,
        Err(c) => {
            result.quiesced = c == a::driver_heap_error_stale;
            return result;
        }
    };
    if !s.closing || s.pending_creates != 0 || s.pending_releases != 0 {
        result.quiesced = false;
        return result;
    }
    for _ in 0..s.allocations {
        let mut ticket = MaybeUninit::uninit();
        let mut present = false;
        {
            let _g = RuntimeGuard::acquire();
            let c = unsafe {
                m::r4os_rust_driver_memory_cleanup(
                    owners(),
                    owner,
                    ticket.as_mut_ptr(),
                    &mut present,
                )
            };
            if c != 0 {
                result.quiesced = false;
                return result;
            }
        }
        if !present {
            break;
        }
        let mut ticket = unsafe { ticket.assume_init() };
        let bytes = ticket.bytes;
        if release_backing(&mut ticket) != a::driver_heap_ok {
            result.quiesced = false;
            return result;
        }
        result.released += 1;
        result.bytes += bytes;
    }
    let _g = RuntimeGuard::acquire();
    if unsafe { m::r4os_rust_driver_memory_mutate(owners(), 2, owner, 0) } != 0 {
        result.quiesced = false
    }
    result
}
/// # Safety
/// Aligned private C records are resident and exclusive under the enclosing
/// driver owner. Backend callbacks preserve that owner and exact record lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_heap_cleanup(owner: u32, out: *mut Cleanup) {
    unsafe { ptr::write(out, cleanup(owner)) };
}

impl Empty for m::Owner {
    const EMPTY: Self = m::Owner {
        stats: m::Stats {
            epoch: 0,
            allocations: 0,
            bytes: 0,
            backing_bytes: 0,
            peak_bytes: 0,
            private_allocations: 0,
            allocation_calls: 0,
            allocation_failures: 0,
            releases: 0,
            release_failures: 0,
            pending_creates: 0,
            pending_releases: 0,
            closing: false,
        },
        tree: Tree {
            root: ptr::null_mut(),
            xorshift: 0,
        },
    };
}
