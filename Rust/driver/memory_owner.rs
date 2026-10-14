//! R4D resident CPU allocation metadata. Actual heap allocation/free occurs
//! outside the runtime owner; successful free never dereferences its record.
use crate::driver::treap::{Node, Tree};
use core::{mem::size_of, ptr};
#[repr(C)]
pub struct Record {
    pub node: Node,
    pub epoch: u64,
    pub requested_bytes: u64,
    pub heap_bytes: usize,
    pub payload_offset: usize,
    pub alignment: u32,
    pub private: bool,
    pub release_failed: bool,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Layout {
    pub requested_bytes: u64,
    pub heap_bytes: usize,
    pub payload_offset: usize,
    pub alignment: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Allocation {
    pub handle: u64,
    pub address: usize,
    pub bytes: u64,
    pub alignment: u32,
}
#[repr(C)]
pub struct Create {
    pub epoch: u64,
    pub handle: u64,
    pub layout: Layout,
    pub owner: u32,
    pub active: bool,
}
#[repr(C)]
pub struct Published {
    pub allocation: Allocation,
    pub visible: bool,
}
#[repr(C)]
pub struct Release {
    pub record: *mut Record,
    pub epoch: u64,
    pub handle: u64,
    pub bytes: u64,
    pub heap_bytes: usize,
    pub owner: u32,
    pub private: bool,
    pub active: bool,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Stats {
    pub epoch: u64,
    pub allocations: u64,
    pub bytes: u64,
    pub backing_bytes: u64,
    pub peak_bytes: u64,
    pub private_allocations: u64,
    pub allocation_calls: u64,
    pub allocation_failures: u64,
    pub releases: u64,
    pub release_failures: u64,
    pub pending_creates: u32,
    pub pending_releases: u32,
    pub closing: bool,
}
#[repr(C)]
#[derive(Default)]
pub struct Owner {
    pub stats: Stats,
    pub tree: Tree,
}
const _: () = {
    assert!(size_of::<Record>() == 80);
    assert!(size_of::<Layout>() == 32);
    assert!(size_of::<Create>() == 56);
    assert!(size_of::<Release>() == 48);
    assert!(size_of::<Stats>() == 96);
    assert!(size_of::<Owner>() == 112);
};
// 1 Invalid, 2 Owner, 3 Stale, 4 Closed, 5 Busy, 6 Exhausted, 7 Overflow.
unsafe fn slot<'a>(owners: *mut Owner, capacity: usize, id: u32) -> Result<&'a mut Owner, u8> {
    if id == 0 || id as usize > capacity {
        return Err(2);
    }
    Ok(unsafe { &mut *owners.add((id - 1) as usize) })
}
unsafe fn current<'a>(owners: *mut Owner, capacity: usize, id: u32) -> Result<&'a mut Owner, u8> {
    let entry = unsafe { slot(owners, capacity, id) }?;
    if entry.stats.epoch == 0 {
        return Err(3);
    }
    Ok(entry)
}
unsafe fn matching<'a>(
    owners: *mut Owner,
    capacity: usize,
    id: u32,
    epoch: u64,
) -> Result<&'a mut Owner, u8> {
    let entry = unsafe { current(owners, capacity, id) }?;
    if entry.stats.epoch != epoch {
        return Err(3);
    }
    Ok(entry)
}
fn status(result: Result<(), u8>) -> u8 {
    result.err().unwrap_or(0)
}
/// # Safety
/// Aligned private C records are resident and exclusive under the enclosing
/// driver owner. Backend callbacks preserve that owner and exact record lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_memory_layout(
    bytes: u64,
    alignment: u32,
    output: *mut Layout,
) -> u8 {
    if bytes == 0 || !alignment.is_power_of_two() {
        return 1;
    }
    let alignment = alignment.max(16);
    let offset = (size_of::<Record>() + alignment as usize - 1) & !(alignment as usize - 1);
    let Some(heap_bytes) = (bytes as usize).checked_add(offset) else {
        return 7;
    };
    unsafe {
        ptr::write(
            output,
            Layout {
                requested_bytes: bytes,
                heap_bytes,
                payload_offset: offset,
                alignment,
            },
        )
    };
    0
}
/// # Safety
/// Aligned private C records are resident and exclusive under the enclosing
/// driver owner. Backend callbacks preserve that owner and exact record lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_memory_mutate(
    owners: *mut Owner,
    cap: usize,
    operation: u8,
    id: u32,
    epoch: u64,
) -> u8 {
    status((|| {
        match operation {
            0 => {
                if epoch == 0 {
                    return Err(1);
                }
                let e = unsafe { slot(owners, cap, id) }?;
                if e.stats.epoch != 0 {
                    if e.stats.epoch != epoch {
                        return Err(5);
                    }
                    if e.stats.closing {
                        return Err(4);
                    }
                    return Ok(());
                }
                *e = Owner {
                    stats: Stats {
                        epoch,
                        ..Stats::default()
                    },
                    tree: Tree::default(),
                };
            }
            1 => {
                if let Ok(e) = unsafe { current(owners, cap, id) } {
                    e.stats.closing = true
                }
            }
            2 => {
                let e = unsafe { slot(owners, cap, id) }?;
                if e.stats.epoch == 0 {
                    return Ok(());
                }
                if !e.stats.closing
                    || e.stats.allocations != 0
                    || e.stats.pending_creates != 0
                    || e.stats.pending_releases != 0
                {
                    return Err(5);
                }
                assert!(e.tree.root.is_null() && e.stats.bytes == 0 && e.stats.backing_bytes == 0);
                *e = Owner::default();
            }
            _ => unreachable!(),
        };
        Ok(())
    })())
}
/// # Safety
/// Aligned private C records are resident and exclusive under the enclosing
/// driver owner. Backend callbacks preserve that owner and exact record lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_memory_snapshot(
    owners: *mut Owner,
    cap: usize,
    id: u32,
    out: *mut Stats,
) -> u8 {
    status((|| {
        let e = unsafe { current(owners, cap, id) }?;
        unsafe { ptr::write(out, e.stats) };
        Ok(())
    })())
}
/// # Safety
/// Aligned private C records are resident and exclusive under the enclosing
/// driver owner. Backend callbacks preserve that owner and exact record lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_memory_begin(
    owners: *mut Owner,
    cap: usize,
    next: *mut u64,
    id: u32,
    layout: *const Layout,
    out: *mut Create,
) -> u8 {
    status((|| {
        let e = unsafe { current(owners, cap, id) }?;
        if e.stats.closing {
            return Err(4);
        }
        let handle = unsafe { *next };
        if handle == 0 {
            return Err(6);
        }
        if e.stats.pending_creates == u32::MAX {
            return Err(5);
        }
        unsafe { *next = handle.checked_add(1).unwrap_or(0) };
        e.stats.pending_creates += 1;
        e.stats.allocation_calls = e.stats.allocation_calls.saturating_add(1);
        unsafe {
            ptr::write(
                out,
                Create {
                    owner: id,
                    epoch: e.stats.epoch,
                    handle,
                    layout: ptr::read(layout),
                    active: true,
                },
            )
        };
        Ok(())
    })())
}
/// # Safety
/// Aligned private C records are resident and exclusive under the enclosing
/// driver owner. Backend callbacks preserve that owner and exact record lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_memory_abort(
    owners: *mut Owner,
    cap: usize,
    create: *mut Create,
) -> u8 {
    status((|| {
        let c = unsafe { &mut *create };
        if !c.active {
            return Err(3);
        }
        let e = unsafe { matching(owners, cap, c.owner, c.epoch) }?;
        assert!(e.stats.pending_creates != 0);
        e.stats.pending_creates -= 1;
        e.stats.allocation_failures = e.stats.allocation_failures.saturating_add(1);
        c.active = false;
        Ok(())
    })())
}
/// # Safety
/// Aligned private C records are resident and exclusive under the enclosing
/// driver owner. Backend callbacks preserve that owner and exact record lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_memory_adopt(
    owners: *mut Owner,
    cap: usize,
    create: *mut Create,
    backing: *mut u8,
    length: usize,
    out: *mut Published,
) -> u8 {
    status((|| {
        let c = unsafe { &mut *create };
        if !c.active {
            return Err(3);
        }
        let e = unsafe { matching(owners, cap, c.owner, c.epoch) }?;
        let l = c.layout;
        assert!(length == l.heap_bytes && (backing as usize).is_multiple_of(l.alignment as usize));
        let r = backing.cast::<Record>();
        // Node bytes are initialized by insertion before any tree dereference.
        unsafe {
            ptr::write(ptr::addr_of_mut!((*r).epoch), c.epoch);
            ptr::write(ptr::addr_of_mut!((*r).requested_bytes), l.requested_bytes);
            ptr::write(ptr::addr_of_mut!((*r).heap_bytes), l.heap_bytes);
            ptr::write(ptr::addr_of_mut!((*r).payload_offset), l.payload_offset);
            ptr::write(ptr::addr_of_mut!((*r).alignment), l.alignment);
            ptr::write(ptr::addr_of_mut!((*r).private), e.stats.closing);
            ptr::write(ptr::addr_of_mut!((*r).release_failed), false);
            assert!(e.tree.find(c.handle).is_null());
            e.tree.insert(c.handle, ptr::addr_of_mut!((*r).node));
        }
        e.stats.pending_creates -= 1;
        e.stats.allocations += 1;
        e.stats.bytes += l.requested_bytes;
        e.stats.backing_bytes += l.heap_bytes as u64;
        e.stats.peak_bytes = e.stats.peak_bytes.max(e.stats.bytes);
        if e.stats.closing {
            e.stats.private_allocations += 1;
            e.stats.allocation_failures = e.stats.allocation_failures.saturating_add(1)
        }
        c.active = false;
        unsafe {
            ptr::write(
                out,
                Published {
                    allocation: Allocation {
                        handle: c.handle,
                        address: backing as usize + l.payload_offset,
                        bytes: l.requested_bytes,
                        alignment: l.alignment,
                    },
                    visible: !e.stats.closing,
                },
            )
        };
        Ok(())
    })())
}
unsafe fn begin_release(
    owners: *mut Owner,
    cap: usize,
    id: u32,
    handle: u64,
    out: *mut Release,
) -> Result<(), u8> {
    if handle == 0 {
        return Err(1);
    }
    let e = unsafe { current(owners, cap, id) }?;
    if e.stats.pending_releases == u32::MAX {
        return Err(5);
    }
    let node = unsafe { e.tree.find(handle) };
    if node.is_null() {
        return Err(3);
    }
    let r = node.cast::<Record>();
    unsafe {
        assert!((*r).epoch == e.stats.epoch);
        ptr::write(
            out,
            Release {
                owner: id,
                epoch: (*r).epoch,
                handle,
                record: r,
                bytes: (*r).requested_bytes,
                heap_bytes: (*r).heap_bytes,
                private: (*r).private,
                active: true,
            },
        );
        e.tree.remove(node)
    };
    e.stats.pending_releases += 1;
    Ok(())
}
/// # Safety
/// Aligned private C records are resident and exclusive under the enclosing
/// driver owner. Backend callbacks preserve that owner and exact record lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_memory_release(
    owners: *mut Owner,
    cap: usize,
    id: u32,
    handle: u64,
    out: *mut Release,
) -> u8 {
    status(unsafe { begin_release(owners, cap, id, handle, out) })
}
/// # Safety
/// Aligned private C records are resident and exclusive under the enclosing
/// driver owner. Backend callbacks preserve that owner and exact record lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_memory_finish_release(
    owners: *mut Owner,
    cap: usize,
    release: *mut Release,
    success: bool,
) -> u8 {
    status((|| {
        let r = unsafe { &mut *release };
        if !r.active {
            return Err(3);
        }
        let e = unsafe { matching(owners, cap, r.owner, r.epoch) }?;
        assert!(e.stats.pending_releases != 0);
        e.stats.pending_releases -= 1;
        if success {
            e.stats.allocations -= 1;
            e.stats.bytes -= r.bytes;
            e.stats.backing_bytes -= r.heap_bytes as u64;
            if r.private {
                e.stats.private_allocations -= 1
            }
            e.stats.releases = e.stats.releases.saturating_add(1)
        } else {
            unsafe {
                (*r.record).release_failed = true;
                assert!(e.tree.find(r.handle).is_null());
                e.tree.insert(r.handle, ptr::addr_of_mut!((*r.record).node))
            };
            e.stats.release_failures = e.stats.release_failures.saturating_add(1)
        }
        r.active = false;
        Ok(())
    })())
}
/// # Safety
/// Aligned private C records are resident and exclusive under the enclosing
/// driver owner. Backend callbacks preserve that owner and exact record lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_memory_cleanup(
    owners: *mut Owner,
    cap: usize,
    id: u32,
    out: *mut Release,
    present: *mut bool,
) -> u8 {
    status((|| {
        let e = unsafe { current(owners, cap, id) }?;
        if !e.stats.closing || e.stats.pending_creates != 0 || e.stats.pending_releases != 0 {
            return Err(5);
        }
        let node = unsafe { e.tree.min() };
        unsafe { *present = !node.is_null() };
        if node.is_null() {
            return Ok(());
        }
        let key = unsafe { (*node).key };
        unsafe { begin_release(owners, cap, id, key, out) }
    })())
}
