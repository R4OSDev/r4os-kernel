//! Dynamic R4D semaphore metadata, independent of queue waits/backing free.
use crate::driver::owner_table::Rows;
use crate::driver::treap::{Node, Tree};
use core::{mem::size_of, ptr};
#[repr(C)]
pub struct Record {
    pub node: Node,
    pub epoch: u64,
    pub handle: u64,
    pub owner: u32,
    pub acquires: u32,
    pub private: bool,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Stats {
    pub epoch: u64,
    pub records: u64,
    pub private: u64,
    pub creates: u64,
    pub create_failures: u64,
    pub destroys: u64,
    pub destroy_failures: u64,
    pub pending_creates: u32,
    pub pending_destroys: u32,
    pub acquires: u32,
    pub closing: bool,
}
#[repr(C)]
#[derive(Default)]
pub struct Owner {
    pub stats: Stats,
    pub tree: Tree,
}
#[repr(C)]
pub struct Create {
    pub epoch: u64,
    pub handle: u64,
    pub owner: u32,
    pub active: bool,
}
#[repr(C)]
pub struct Destroy {
    pub record: *mut Record,
    pub epoch: u64,
    pub handle: u64,
    pub owner: u32,
    pub private: bool,
    pub active: bool,
}
const _: () = {
    assert!(size_of::<Record>() == 72);
    assert!(size_of::<Stats>() == 72);
    assert!(size_of::<Owner>() == 88);
    assert!(size_of::<Create>() == 24);
    assert!(size_of::<Destroy>() == 32);
};
unsafe fn slot<'a>(rows: Rows<Owner>, id: u32) -> Result<&'a mut Owner, u8> {
    if id == 0 {
        return Err(2);
    }
    let row = unsafe { rows.get((id - 1) as usize) };
    if row.is_null() {
        return Err(2);
    }
    Ok(unsafe { &mut *row })
}
unsafe fn current<'a>(rows: Rows<Owner>, id: u32) -> Result<&'a mut Owner, u8> {
    let e = unsafe { slot(rows, id) }?;
    if e.stats.epoch == 0 {
        return Err(3);
    }
    Ok(e)
}
unsafe fn matching<'a>(rows: Rows<Owner>, id: u32, epoch: u64) -> Result<&'a mut Owner, u8> {
    let e = unsafe { current(rows, id) }?;
    if e.stats.epoch != epoch {
        return Err(3);
    }
    Ok(e)
}
fn status(result: Result<(), u8>) -> u8 {
    result.err().unwrap_or(0)
}
unsafe fn lookup(rows: Rows<Owner>, id: u32, handle: u64) -> Result<*mut Record, u8> {
    if handle == 0 {
        return Err(1);
    }
    let e = unsafe { current(rows, id) }?;
    let n = unsafe { e.tree.find(handle) };
    if n.is_null() {
        return Err(3);
    }
    let r = n.cast::<Record>();
    unsafe { assert!((*r).owner == id && (*r).epoch == e.stats.epoch) };
    Ok(r)
}
/// # Safety
/// Private C records stay aligned and resident under the runtime owner. Output
/// headers/buffers are valid caller storage; exact wait/free tickets retain identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_semaphore_owner_mutate(
    rows: Rows<Owner>,
    id: u32,
    epoch: u64,
    op: u8,
) -> u8 {
    status((|| {
        match op {
            0 => {
                if epoch == 0 {
                    return Err(1);
                }
                let e = unsafe { slot(rows, id) }?;
                if e.stats.epoch != 0 {
                    return Err(5);
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
                if let Ok(e) = unsafe { current(rows, id) } {
                    e.stats.closing = true
                }
            }
            2 => {
                let e = unsafe { current(rows, id) }?;
                if !e.stats.closing
                    || e.stats.records != 0
                    || e.stats.pending_creates != 0
                    || e.stats.pending_destroys != 0
                    || e.stats.acquires != 0
                {
                    return Err(5);
                }
                *e = Owner::default()
            }
            _ => unreachable!(),
        };
        Ok(())
    })())
}
/// # Safety
/// Private C records stay aligned and resident under the runtime owner. Output
/// headers/buffers are valid caller storage; exact wait/free tickets retain identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_semaphore_owner_stats(
    rows: Rows<Owner>,
    id: u32,
    out: *mut Stats,
) -> u8 {
    status((|| {
        let e = unsafe { current(rows, id) }?;
        unsafe { ptr::write(out, e.stats) };
        Ok(())
    })())
}
/// # Safety
/// Private C records stay aligned and resident under the runtime owner. Output
/// headers/buffers are valid caller storage; exact wait/free tickets retain identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_semaphore_owner_begin(
    rows: Rows<Owner>,
    next: *mut u64,
    id: u32,
    out: *mut Create,
) -> u8 {
    status((|| {
        let e = unsafe { current(rows, id) }?;
        if e.stats.closing {
            return Err(4);
        }
        let h = unsafe { *next };
        if h == 0 {
            return Err(6);
        }
        if e.stats.pending_creates == u32::MAX {
            return Err(5);
        }
        unsafe { *next = h.checked_add(1).unwrap_or(0) };
        e.stats.pending_creates += 1;
        e.stats.creates = e.stats.creates.saturating_add(1);
        unsafe {
            ptr::write(
                out,
                Create {
                    owner: id,
                    epoch: e.stats.epoch,
                    handle: h,
                    active: true,
                },
            )
        };
        Ok(())
    })())
}
/// # Safety
/// Private C records stay aligned and resident under the runtime owner. Output
/// headers/buffers are valid caller storage; exact wait/free tickets retain identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_semaphore_owner_abort(rows: Rows<Owner>, c: *mut Create) -> u8 {
    status((|| {
        let c = unsafe { &mut *c };
        if !c.active {
            return Err(3);
        }
        let e = unsafe { matching(rows, c.owner, c.epoch) }?;
        assert!(e.stats.pending_creates != 0);
        e.stats.pending_creates -= 1;
        e.stats.create_failures = e.stats.create_failures.saturating_add(1);
        c.active = false;
        Ok(())
    })())
}
/// # Safety
/// Private C records stay aligned and resident under the runtime owner. Output
/// headers/buffers are valid caller storage; exact wait/free tickets retain identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_semaphore_owner_adopt(
    rows: Rows<Owner>,
    c: *mut Create,
    r: *mut Record,
    visible: *mut bool,
) -> u8 {
    status((|| {
        let c = unsafe { &mut *c };
        if !c.active {
            return Err(3);
        }
        let e = unsafe { matching(rows, c.owner, c.epoch) }?;
        unsafe {
            assert!((*r).owner == 0 && e.stats.pending_creates != 0);
            (*r).owner = c.owner;
            (*r).epoch = c.epoch;
            (*r).handle = c.handle;
            (*r).private = e.stats.closing;
            (*r).acquires = 0;
            assert!(e.tree.find(c.handle).is_null());
            e.tree.insert(c.handle, ptr::addr_of_mut!((*r).node));
        }
        e.stats.records += 1;
        e.stats.pending_creates -= 1;
        if e.stats.closing {
            e.stats.private += 1;
            e.stats.create_failures = e.stats.create_failures.saturating_add(1)
        }
        c.active = false;
        unsafe { *visible = !e.stats.closing };
        Ok(())
    })())
}
/// # Safety
/// Private C records stay aligned and resident under the runtime owner. Output
/// headers/buffers are valid caller storage; exact wait/free tickets retain identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_semaphore_owner_lookup(
    rows: Rows<Owner>,
    id: u32,
    handle: u64,
    op: u8,
    out: *mut *mut Record,
) -> u8 {
    status((|| {
        let r = unsafe { lookup(rows, id, handle) }?;
        if op != 0 && unsafe { (*r).private } {
            return Err(3);
        }
        if op == 2 {
            let e = unsafe { matching(rows, id, (*r).epoch) }?;
            if unsafe { (*r).acquires == u32::MAX } || e.stats.acquires == u32::MAX {
                return Err(5);
            }
            unsafe { (*r).acquires += 1 };
            e.stats.acquires += 1;
        }
        unsafe { ptr::write(out, r) };
        Ok(())
    })())
}
/// # Safety
/// Private C records stay aligned and resident under the runtime owner. Output
/// headers/buffers are valid caller storage; exact wait/free tickets retain identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_semaphore_owner_acquired(
    rows: Rows<Owner>,
    r: *mut Record,
) -> u8 {
    status((|| {
        let e = unsafe { matching(rows, (*r).owner, (*r).epoch) }?;
        if unsafe { (*r).acquires == 0 } {
            return Err(3);
        }
        unsafe { (*r).acquires -= 1 };
        e.stats.acquires -= 1;
        Ok(())
    })())
}
/// # Safety
/// Private C records stay aligned and resident under the runtime owner. Output
/// headers/buffers are valid caller storage; exact wait/free tickets retain identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_semaphore_owner_destroy(
    rows: Rows<Owner>,
    id: u32,
    handle: u64,
    out: *mut Destroy,
) -> u8 {
    status((|| {
        let r = unsafe { lookup(rows, id, handle) }?;
        let e = unsafe { matching(rows, id, (*r).epoch) }?;
        if unsafe { (*r).acquires != 0 } || e.stats.pending_destroys == u32::MAX {
            return Err(5);
        }
        unsafe {
            ptr::write(
                out,
                Destroy {
                    record: r,
                    owner: id,
                    epoch: (*r).epoch,
                    handle,
                    private: (*r).private,
                    active: true,
                },
            );
            e.tree.remove(ptr::addr_of_mut!((*r).node))
        };
        e.stats.pending_destroys += 1;
        Ok(())
    })())
}
/// # Safety
/// Private C records stay aligned and resident under the runtime owner. Output
/// headers/buffers are valid caller storage; exact wait/free tickets retain identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_semaphore_owner_finish_destroy(
    rows: Rows<Owner>,
    t: *mut Destroy,
    success: bool,
) -> u8 {
    status((|| {
        let t = unsafe { &mut *t };
        if !t.active {
            return Err(3);
        }
        let e = unsafe { matching(rows, t.owner, t.epoch) }?;
        assert!(e.stats.pending_destroys != 0);
        e.stats.pending_destroys -= 1;
        if success {
            e.stats.records -= 1;
            if t.private {
                e.stats.private -= 1
            }
            e.stats.destroys = e.stats.destroys.saturating_add(1)
        } else {
            unsafe {
                assert!(e.tree.find(t.handle).is_null());
                e.tree.insert(t.handle, ptr::addr_of_mut!((*t.record).node))
            };
            e.stats.destroy_failures = e.stats.destroy_failures.saturating_add(1)
        }
        t.active = false;
        Ok(())
    })())
}
/// # Safety
/// Private C records stay aligned and resident under the runtime owner. Output
/// headers/buffers are valid caller storage; exact wait/free tickets retain identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_semaphore_owner_first(
    rows: Rows<Owner>,
    id: u32,
    out: *mut *mut Record,
) -> u8 {
    status((|| {
        let e = unsafe { current(rows, id) }?;
        unsafe { ptr::write(out, e.tree.min().cast()) };
        Ok(())
    })())
}
