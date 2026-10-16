//! Fixed private identities and status transport. These records create no
//! ownership: the admitted BO/provider/driver epoch remains authoritative.
use super::layout;
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Owner {
    pub kind: u32,
    pub id: u64,
    pub generation: u64,
}
impl Owner {
    pub const EMPTY: Self = Self {
        kind: 0,
        id: 0,
        generation: 0,
    };
    pub fn valid(self) -> bool {
        self.id != 0 && self.generation != 0
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Handle {
    pub id: u32,
    pub generation: u64,
}
impl Handle {
    pub const EMPTY: Self = Self {
        id: 0,
        generation: 0,
    };
}
pub fn persistent_cpu(access: u32) -> bool {
    matches!(access, 8 | 9)
}
pub fn is_cpu(access: u32) -> bool {
    matches!(access, 0 | 1) || persistent_cpu(access)
}
pub fn queued(access: u32) -> bool {
    matches!(access, 6 | 7)
}
pub fn execution(access: u32) -> bool {
    matches!(access, 2 | 3) || queued(access)
}
pub fn writes(access: u32) -> bool {
    matches!(access, 1 | 3 | 7 | 9)
}
pub fn conflicts(first: u32, second: u32) -> bool {
    // Address lifetime and explicit CPU/GPU synchronization remain independent.
    // Ordinary CPU maps and scanout retain the original writer exclusions.
    if first == 5 || second == 5 {
        return false;
    }
    if (persistent_cpu(first) && execution(second)) || (persistent_cpu(second) && execution(first))
    {
        return false;
    }
    if queued(first) && queued(second) {
        return false;
    }
    writes(first) || writes(second)
}
/// # Safety
/// Caller retains one initialized identity through this synchronous query.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_owner_valid(p: *const Owner) -> bool {
    unsafe { p.read() }.valid()
}
/// # Safety
/// Caller retains both initialized identity records through the comparison.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_owner_equal(a: *const Owner, b: *const Owner) -> bool {
    unsafe { a.read() == b.read() }
}
/// # Safety
/// Caller retains both initialized private handle records through comparison.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_handle_equal(a: *const Handle, b: *const Handle) -> bool {
    unsafe { a.read() == b.read() }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_gfx_access_predicate(access: u32, kind: u8) -> bool {
    match kind {
        0 => is_cpu(access),
        1 => !is_cpu(access),
        2 => persistent_cpu(access),
        3 => execution(access),
        4 => queued(access),
        5 => writes(access),
        _ => unreachable!(),
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_gfx_access_conflicts(a: u32, b: u32) -> bool {
    conflicts(a, b)
}
#[repr(i32)]
#[derive(Clone, Copy, Debug)]
pub enum Error {
    Invalid = 1,
    Overflow = 2,
    Unsupported = 3,
    Exhausted = 4,
    Budget = 5,
    Capacity = 6,
    Stale = 7,
    Busy = 8,
    Closed = 9,
    WrongOwner = 10,
    OutOfMemory = 11,
    Unavailable = 12,
    WaitTimeout = 13,
    WaitCancelled = 14,
}
impl From<layout::Error> for Error {
    fn from(value: layout::Error) -> Self {
        match value {
            layout::Error::Invalid => Self::Invalid,
            layout::Error::Overflow => Self::Overflow,
            layout::Error::Unsupported => Self::Unsupported,
        }
    }
}
const _: () = {
    assert!(core::mem::size_of::<Owner>() == 24);
    assert!(core::mem::align_of::<Owner>() == 8);
    assert!(core::mem::offset_of!(Owner, kind) == 0);
    assert!(core::mem::offset_of!(Owner, id) == 8);
    assert!(core::mem::offset_of!(Owner, generation) == 16);
    assert!(core::mem::size_of::<Handle>() == 16);
    assert!(core::mem::offset_of!(Handle, id) == 0);
    assert!(core::mem::offset_of!(Handle, generation) == 8);
};
