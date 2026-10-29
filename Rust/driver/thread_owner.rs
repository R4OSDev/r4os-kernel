//! Dynamic exact R4D thread ownership; tasks, FPU, waits and real backing
//! are controlled by the enclosing runtime, outside this resident index.
use crate::driver::owner_table::Rows;
use crate::driver::treap::{Node, Tree};
use core::{mem::size_of, ptr};
#[repr(C)]
pub struct Record {
    pub node: Node,
    pub epoch: u64,
    pub handle: u64,
    pub owner: u32,
    pub phase: u32,
    pub references: u32,
    pub result: i32,
    pub private: bool,
    pub stop_requested: bool,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Stats {
    pub epoch: u64,
    pub records: u64,
    pub active: u64,
    pub completed: u64,
    pub private: u64,
    pub starts: u64,
    pub start_failures: u64,
    pub releases: u64,
    pub release_failures: u64,
    pub pending_creates: u32,
    pub pending_releases: u32,
    pub leases: u32,
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
pub struct Removal {
    pub record: *mut Record,
    pub epoch: u64,
    pub handle: u64,
    pub owner: u32,
    pub private: bool,
    pub active: bool,
}
const _: () = {
    assert!(size_of::<Record>() == 80);
    assert!(size_of::<Stats>() == 88);
    assert!(size_of::<Owner>() == 104);
    assert!(size_of::<Create>() == 24);
    assert!(size_of::<Removal>() == 32);
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
pub unsafe extern "C" fn r4os_rust_thread_owner_mutate(
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
                if let Ok(e) = unsafe { current(rows, id) } {
                    e.stats.closing = true
                }
            }
            2 => {
                let e = unsafe { current(rows, id) }?;
                if !e.stats.closing
                    || e.stats.records != 0
                    || e.stats.pending_creates != 0
                    || e.stats.pending_releases != 0
                    || e.stats.leases != 0
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
pub unsafe extern "C" fn r4os_rust_thread_owner_stats(
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
pub unsafe extern "C" fn r4os_rust_thread_owner_begin(
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
        e.stats.starts = e.stats.starts.saturating_add(1);
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
pub unsafe extern "C" fn r4os_rust_thread_owner_abort(rows: Rows<Owner>, c: *mut Create) -> u8 {
    status((|| {
        let c = unsafe { &mut *c };
        if !c.active {
            return Err(3);
        }
        let e = unsafe { matching(rows, c.owner, c.epoch) }?;
        assert!(e.stats.pending_creates != 0);
        e.stats.pending_creates -= 1;
        e.stats.start_failures = e.stats.start_failures.saturating_add(1);
        c.active = false;
        Ok(())
    })())
}
/// # Safety
/// Private C records stay aligned and resident under the runtime owner. Output
/// headers/buffers are valid caller storage; exact wait/free tickets retain identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_thread_owner_adopt(
    rows: Rows<Owner>,
    c: *mut Create,
    r: *mut Record,
) -> u8 {
    status((|| {
        let c = unsafe { &mut *c };
        if !c.active {
            return Err(3);
        }
        let e = unsafe { matching(rows, c.owner, c.epoch) }?;
        unsafe {
            assert!((*r).owner == 0 && (*r).references == 0);
            (*r).owner = c.owner;
            (*r).epoch = c.epoch;
            (*r).handle = c.handle;
            (*r).phase = 0;
            (*r).private = true;
            assert!(e.tree.find(c.handle).is_null());
            e.tree.insert(c.handle, ptr::addr_of_mut!((*r).node))
        };
        e.stats.records += 1;
        e.stats.private += 1;
        c.active = false;
        Ok(())
    })())
}
// 0 prepared, 1 failed ready publication, 2 enter, 3 complete,
// 4 stopping, 5 release a metadata reference.
/// # Safety
/// Private C records stay aligned and resident under the runtime owner. Output
/// headers/buffers are valid caller storage; exact wait/free tickets retain identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_thread_owner_record(
    rows: Rows<Owner>,
    r: *mut Record,
    op: u8,
    task_created: bool,
    result: i32,
    out: *mut bool,
) -> u8 {
    status((|| {
        let e = unsafe { matching(rows, (*r).owner, (*r).epoch) }?;
        unsafe {
            match op {
                0 => {
                    if (*r).phase != 0 {
                        return Err(3);
                    }
                    assert!(e.stats.pending_creates != 0);
                    e.stats.pending_creates -= 1;
                    if !task_created || e.stats.closing {
                        (*r).phase = 3;
                        (*r).result = result;
                        (*r).stop_requested = true;
                        e.stats.start_failures = e.stats.start_failures.saturating_add(1);
                        e.stats.completed += 1;
                        *out = false;
                    } else {
                        (*r).phase = 1;
                        (*r).private = false;
                        e.stats.private -= 1;
                        e.stats.active += 1;
                        *out = true;
                    }
                }
                1 => {
                    if (*r).phase != 1 {
                        return Err(3);
                    }
                    (*r).phase = 3;
                    (*r).result = result;
                    (*r).private = true;
                    (*r).stop_requested = true;
                    e.stats.active -= 1;
                    e.stats.completed += 1;
                    e.stats.private += 1;
                    e.stats.start_failures = e.stats.start_failures.saturating_add(1);
                }
                2 => {
                    if (*r).phase != 1 {
                        return Err(3);
                    }
                    (*r).phase = 2;
                    *out = !((*r).stop_requested || e.stats.closing);
                }
                3 => {
                    if (*r).phase != 2 {
                        return Err(3);
                    }
                    (*r).phase = 3;
                    (*r).result = result;
                    e.stats.active -= 1;
                    e.stats.completed += 1;
                }
                4 => {
                    *out = (*r).stop_requested || e.stats.closing;
                }
                5 => {
                    if (*r).references == 0 {
                        return Err(3);
                    }
                    (*r).references -= 1;
                    e.stats.leases -= 1;
                }
                _ => unreachable!(),
            }
        };
        Ok(())
    })())
}
/// # Safety
/// Private C records stay aligned and resident under the runtime owner. Output
/// headers/buffers are valid caller storage; exact wait/free tickets retain identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_thread_owner_lookup(
    rows: Rows<Owner>,
    id: u32,
    handle: u64,
    op: u8,
    out: *mut *mut Record,
) -> u8 {
    status((|| {
        let r = unsafe { lookup(rows, id, handle) }?;
        if op == 1 {
            unsafe { (*r).stop_requested = true }
        }
        if op == 2 {
            let e = unsafe { matching(rows, id, (*r).epoch) }?;
            if unsafe { (*r).phase == 0 || (*r).references == u32::MAX }
                || e.stats.leases == u32::MAX
            {
                return Err(5);
            }
            unsafe { (*r).references += 1 };
            e.stats.leases += 1;
        }
        unsafe { ptr::write(out, r) };
        Ok(())
    })())
}
/// # Safety
/// Private C records stay aligned and resident under the runtime owner. Output
/// headers/buffers are valid caller storage; exact wait/free tickets retain identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_thread_owner_remove(
    rows: Rows<Owner>,
    id: u32,
    handle: u64,
    out: *mut Removal,
) -> u8 {
    status((|| {
        let r = unsafe { lookup(rows, id, handle) }?;
        let e = unsafe { matching(rows, id, (*r).epoch) }?;
        if unsafe { (*r).phase != 3 || (*r).references != 0 }
            || e.stats.pending_releases == u32::MAX
        {
            return Err(5);
        }
        unsafe {
            ptr::write(
                out,
                Removal {
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
        e.stats.pending_releases += 1;
        Ok(())
    })())
}
/// # Safety
/// Private C records stay aligned and resident under the runtime owner. Output
/// headers/buffers are valid caller storage; exact wait/free tickets retain identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_thread_owner_finish_remove(
    rows: Rows<Owner>,
    t: *mut Removal,
    success: bool,
) -> u8 {
    status((|| {
        let t = unsafe { &mut *t };
        if !t.active {
            return Err(3);
        }
        let e = unsafe { matching(rows, t.owner, t.epoch) }?;
        assert!(e.stats.pending_releases != 0);
        e.stats.pending_releases -= 1;
        if success {
            e.stats.records -= 1;
            e.stats.completed -= 1;
            if t.private {
                e.stats.private -= 1
            }
            e.stats.releases = e.stats.releases.saturating_add(1)
        } else {
            unsafe {
                assert!(e.tree.find(t.handle).is_null());
                e.tree.insert(t.handle, ptr::addr_of_mut!((*t.record).node))
            };
            e.stats.release_failures = e.stats.release_failures.saturating_add(1)
        }
        t.active = false;
        Ok(())
    })())
}
/// # Safety
/// Private C records stay aligned and resident under the runtime owner. Output
/// headers/buffers are valid caller storage; exact wait/free tickets retain identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_thread_owner_after(
    rows: Rows<Owner>,
    id: u32,
    cursor: u64,
    out: *mut *mut Record,
) -> u8 {
    status((|| {
        let e = unsafe { current(rows, id) }?;
        let mut node = e.tree.root;
        let mut found = ptr::null_mut();
        unsafe {
            while !node.is_null() {
                if (*node).key > cursor {
                    found = node;
                    node = (*node).children[0]
                } else {
                    node = (*node).children[1]
                }
            }
            ptr::write(out, found.cast())
        };
        Ok(())
    })())
}
