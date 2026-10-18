//! Owner-bound MMIO windows; the existing driver execution guard serializes
//! complete operations. No Rust borrow is retained over a backend callback.
use core::{ffi::c_void, mem::size_of, ptr};
const PAGE: u64 = 4096;
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Owner {
    pub id: u64,
    pub generation: u64,
    pub kind: u8,
}
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Handle {
    pub generation: u64,
    pub id: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Request {
    pub resource_base: u64,
    pub resource_bytes: u64,
    pub offset: u64,
    pub bytes: u64,
    pub policy: u8,
    pub prefetchable: bool,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Extent {
    pub physical: u64,
    pub bytes: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Window {
    pub handle: Handle,
    pub cpu: u64,
    pub physical: u64,
    pub bytes: u64,
    pub policy: u8,
    pub borrowed: bool,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Record {
    pub owner: Owner,
    pub window: Window,
    pub mapped: u64,
    pub used: bool,
    pub retiring: bool,
}
#[repr(C)]
pub struct State {
    pub records: [Record; 64],
    pub serial: u64,
    pub charged: u64,
    pub budget: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Backend {
    pub context: *mut c_void,
    pub call: unsafe extern "C" fn(*mut c_void, u8, u64, u64, u8, *mut u64) -> bool,
}
const _: () = {
    assert!(size_of::<Owner>() == 24);
    assert!(size_of::<Window>() == 48);
    assert!(size_of::<Record>() == 88);
    assert!(size_of::<State>() == 5656);
    assert!(size_of::<Request>() == 40);
};
// 1 Invalid, 2 Overflow, 3 Unsupported, 4 Busy, 5 Capacity, 6 Stale,
// 7 Exhausted, 8 MapFailed; zero is success.
fn validate(r: Request) -> Result<Extent, u8> {
    if r.resource_base == 0
        || r.resource_bytes == 0
        || r.bytes == 0
        || !(r.resource_base | r.resource_bytes | r.offset | r.bytes).is_multiple_of(PAGE)
        || r.offset > r.resource_bytes
        || r.bytes > r.resource_bytes - r.offset
    {
        return Err(1);
    }
    if r.policy == 0 || (r.policy == 1 && !r.prefetchable) {
        return Err(3);
    }
    r.resource_base.checked_add(r.resource_bytes).ok_or(2)?;
    let physical = r.resource_base.checked_add(r.offset).ok_or(2)?;
    let end = physical.checked_add(r.bytes).ok_or(2)?;
    if end > 1 << 52 {
        return Err(3);
    }
    Ok(Extent {
        physical,
        bytes: r.bytes,
    })
}
unsafe fn effect(b: Backend, op: u8, a: u64, c: u64, policy: u8) -> (bool, u64) {
    let mut value = 0;
    let ok = unsafe { (b.call)(b.context, op, a, c, policy, &mut value) };
    (ok, value)
}
unsafe fn policy(b: Backend, address: u64) -> Option<u8> {
    let (present, value) = unsafe { effect(b, 2, address, 0, 0) };
    present.then_some(value as u8)
}
unsafe fn reclaim(s: *mut State, b: Backend, index: usize) -> bool {
    loop {
        let r = unsafe { ptr::read(ptr::addr_of!((*s).records[index])) };
        if r.mapped == 0 {
            unsafe {
                (*s).charged -= r.window.bytes;
                ptr::write(ptr::addr_of_mut!((*s).records[index]), Record::default())
            };
            return true;
        }
        let offset = r.mapped - PAGE;
        if !unsafe { effect(b, 4, r.window.cpu + offset, 0, 0) }.0 {
            return false;
        }
        unsafe { (*s).records[index].mapped = offset };
    }
}
unsafe fn create(s: *mut State, b: Backend, owner: Owner, r: Request) -> Result<Window, u8> {
    if owner.id == 0 || owner.generation == 0 || owner.kind != 2 {
        return Err(1);
    }
    let e = validate(r)?;
    if unsafe { (*s).charged > (*s).budget || e.bytes > (*s).budget - (*s).charged } {
        return Err(5);
    }
    if !unsafe { effect(b, 0, e.physical, e.bytes, 0) }.0 {
        return Err(3);
    }
    let (present, cpu) = unsafe { effect(b, 1, e.physical, e.bytes, 0) };
    if !present {
        return Err(2);
    }
    let mut slot = None;
    for index in 0..64 {
        let previous = unsafe { ptr::read(ptr::addr_of!((*s).records[index])) };
        if !previous.used {
            slot = slot.or(Some(index));
            continue;
        }
        if e.physical < previous.window.physical + previous.window.bytes
            && previous.window.physical < e.physical + e.bytes
        {
            return Err(4);
        }
    }
    let index = slot.ok_or(5)?;
    if unsafe { (*s).serial == u64::MAX } {
        return Err(7);
    }
    let existing = unsafe { policy(b, cpu) };
    if existing.is_some_and(|p| p != r.policy) {
        return Err(3);
    }
    let mut offset = 0;
    while offset < e.bytes {
        if unsafe { policy(b, cpu + offset) } != existing {
            return Err(3);
        }
        offset += PAGE;
    }
    unsafe { (*s).serial += 1 };
    let window = Window {
        handle: Handle {
            generation: unsafe { (*s).serial },
            id: (index + 1) as u32,
        },
        cpu,
        physical: e.physical,
        bytes: e.bytes,
        policy: r.policy,
        borrowed: existing.is_some(),
    };
    unsafe {
        ptr::write(
            ptr::addr_of_mut!((*s).records[index]),
            Record {
                owner,
                window,
                used: true,
                ..Record::default()
            },
        );
        (*s).charged += e.bytes
    };
    if !window.borrowed {
        loop {
            let mapped = unsafe { (*s).records[index].mapped };
            if mapped == e.bytes {
                break;
            }
            if !unsafe { effect(b, 3, cpu + mapped, e.physical + mapped, r.policy) }.0 {
                unsafe {
                    (*s).records[index].retiring = true;
                    reclaim(s, b, index)
                };
                return Err(8);
            }
            unsafe { (*s).records[index].mapped += PAGE };
        }
    }
    Ok(window)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_mmio_cache_selector(pat: u64, memory_type: u8) -> u8 {
    crate::arch::memory_types::selector(pat, memory_type).unwrap_or(255)
}
/// # Safety
/// Aligned private C records are resident and exclusive under the enclosing
/// driver owner. Backend callbacks preserve that owner and exact record lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mmio_validate(
    request: *const Request,
    output: *mut Extent,
) -> u8 {
    match validate(unsafe { ptr::read(request) }) {
        Ok(e) => {
            unsafe { ptr::write(output, e) };
            0
        }
        Err(code) => code,
    }
}
/// # Safety
/// Aligned private C records are resident and exclusive under the enclosing
/// driver owner. Backend callbacks preserve that owner and exact record lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mmio_create(
    state: *mut State,
    backend: *const Backend,
    owner: *const Owner,
    request: *const Request,
    output: *mut Window,
) -> u8 {
    match unsafe {
        create(
            state,
            ptr::read(backend),
            ptr::read(owner),
            ptr::read(request),
        )
    } {
        Ok(w) => {
            unsafe { ptr::write(output, w) };
            0
        }
        Err(code) => code,
    }
}
/// # Safety
/// Aligned private C records are resident and exclusive under the enclosing
/// driver owner. Backend callbacks preserve that owner and exact record lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mmio_release(
    s: *mut State,
    b: *const Backend,
    owner: *const Owner,
    handle: *const Handle,
    quiesced: bool,
) -> u8 {
    let owner = unsafe { ptr::read(owner) };
    let h = unsafe { ptr::read(handle) };
    if h.id == 0 || h.id > 64 {
        return 6;
    }
    let index = (h.id - 1) as usize;
    let r = unsafe { ptr::read(ptr::addr_of!((*s).records[index])) };
    if !r.used || r.owner != owner || r.window.handle != h {
        return 6;
    }
    if !quiesced {
        return 4;
    }
    unsafe { (*s).records[index].retiring = true };
    if unsafe { reclaim(s, ptr::read(b), index) } {
        0
    } else {
        4
    }
}
/// # Safety
/// Aligned private C records are resident and exclusive under the enclosing
/// driver owner. Backend callbacks preserve that owner and exact record lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mmio_query(
    s: *mut State,
    b: *const Backend,
    owner: *const Owner,
    op: u8,
) -> bool {
    let owner = unsafe { ptr::read(owner) };
    let mut released = true;
    for index in 0..64 {
        let r = unsafe { ptr::read(ptr::addr_of!((*s).records[index])) };
        if !r.used || r.owner != owner {
            continue;
        }
        match op {
            0 => {
                if r.retiring && !unsafe { reclaim(s, ptr::read(b), index) } {
                    released = false
                }
            }
            1 => return true,
            2 => {
                if r.retiring {
                    return true;
                }
            }
            _ => unreachable!(),
        }
    }
    if op == 0 { released } else { false }
}
