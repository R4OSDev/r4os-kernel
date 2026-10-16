//! Exactly two resident catalogue banks. Read leases pin the exact task unwind;
//! a nonblocking writer reservation spans file I/O without a no-sleep owner.
use super::types::*;
use crate::sched::task_context::Unwind;
#[cfg(not(r4os_owner_fixture))]
use crate::{arch::runtime::RuntimeGuard, sched::task_context};
use core::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    ptr,
    sync::atomic::{AtomicU8, AtomicU32, Ordering},
};
struct Banks(UnsafeCell<[CatalogState; 2]>);
// SAFETY: current bank is immutable. A sole writer owns the other bank only
// after observing its exact reader count zero; publication/retirement use SeqCst.
unsafe impl Sync for Banks {}
static BANKS: Banks = Banks(UnsafeCell::new([CatalogState::EMPTY, CatalogState::EMPTY]));
struct Payload(UnsafeCell<MaybeUninit<[[CachedFont; MAX_CATALOG_FONTS]; 2]>>);
// SAFETY: each bank's payload shares the exact same writer/read-lease ownership.
// used is initialized before inspection; a complete decode publishes used=true.
unsafe impl Sync for Payload {}
static PAYLOADS: Payload = Payload(UnsafeCell::new(MaybeUninit::uninit()));
static EMPTY: CatalogState = CatalogState::EMPTY;
static CURRENT: AtomicU8 = AtomicU8::new(0);
static READERS: [AtomicU32; 2] = [const { AtomicU32::new(0) }; 2];
static WRITER: AtomicU8 = AtomicU8::new(0);
fn bank(index: u8) -> *mut CatalogState {
    assert!(index < 2);
    unsafe { BANKS.0.get().cast::<CatalogState>().add(index as usize) }
}
fn payload(index: u8) -> *mut CachedFont {
    assert!(index < 2);
    unsafe {
        PAYLOADS
            .0
            .get()
            .cast::<CachedFont>()
            .add(index as usize * MAX_CATALOG_FONTS)
    }
}
fn enter_unwind() -> Unwind {
    #[cfg(r4os_owner_fixture)]
    {
        Unwind::EMPTY
    }
    #[cfg(not(r4os_owner_fixture))]
    {
        let _guard = RuntimeGuard::acquire();
        unsafe { task_context::enter() }
    }
}
fn leave_unwind(token: Unwind) {
    #[cfg(not(r4os_owner_fixture))]
    {
        let _guard = RuntimeGuard::acquire();
        let _ = unsafe { task_context::leave(token) };
    }
    #[cfg(r4os_owner_fixture)]
    let _ = token;
}
#[repr(C)]
pub struct View {
    pub bank: u8,
    pub unwind: Unwind,
}
impl View {
    pub const EMPTY: Self = Self {
        bank: 255,
        unwind: Unwind::EMPTY,
    };
    pub fn state(&self) -> *const CatalogState {
        if self.bank == 255 {
            ptr::addr_of!(EMPTY)
        } else {
            bank(self.bank)
        }
    }
    pub fn release(&mut self) {
        if self.bank == 255 {
            return;
        }
        assert!(self.bank < 2);
        READERS[self.bank as usize].fetch_sub(1, Ordering::Release);
        leave_unwind(self.unwind);
        self.bank = 255;
    }
}
impl Drop for View {
    fn drop(&mut self) {
        self.release();
    }
}
pub fn acquire() -> View {
    let unwind = enter_unwind();
    if !unwind.admitted() {
        return View::EMPTY;
    }
    loop {
        let selected = CURRENT.load(Ordering::Acquire);
        READERS[selected as usize].fetch_add(1, Ordering::SeqCst);
        if CURRENT.load(Ordering::SeqCst) == selected {
            return View {
                bank: selected,
                unwind,
            };
        }
        READERS[selected as usize].fetch_sub(1, Ordering::Release);
    }
}
#[repr(C)]
pub struct Replacement {
    pub bank: u8,
    pub unwind: Unwind,
    pub owned: bool,
}
impl Replacement {
    pub fn state(&self) -> *mut CatalogState {
        assert!(self.owned, "Original owned font replacement");
        bank(self.bank)
    }
    pub fn commit(&mut self) {
        if !self.owned {
            return;
        }
        let old = CURRENT.load(Ordering::Acquire);
        let mut revision = unsafe { ptr::addr_of!((*bank(old)).revision).read() }.wrapping_add(1);
        if revision == 0 {
            revision = 1
        }
        unsafe { ptr::addr_of_mut!((*self.state()).revision).write(revision) }
        CURRENT.store(self.bank, Ordering::SeqCst);
        self.abort();
    }
    pub fn abort(&mut self) {
        if !self.owned {
            return;
        }
        self.owned = false;
        WRITER.store(0, Ordering::Release);
        leave_unwind(self.unwind);
    }
}
impl Drop for Replacement {
    fn drop(&mut self) {
        self.abort();
    }
}
pub fn begin(preserve: bool) -> Option<Replacement> {
    let unwind = enter_unwind();
    if !unwind.admitted() {
        return None;
    }
    if WRITER
        .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        leave_unwind(unwind);
        return None;
    }
    let old = CURRENT.load(Ordering::Acquire);
    let next = old ^ 1;
    if READERS[next as usize].load(Ordering::SeqCst) != 0 {
        WRITER.store(0, Ordering::Release);
        leave_unwind(unwind);
        return None;
    }
    let source = if preserve {
        bank(old) as *const CatalogState
    } else {
        ptr::addr_of!(EMPTY)
    };
    let target = bank(next);
    // Raw direct resident copies preserve potentially uninitialized inactive cache
    // bytes without materializing a multi-megabyte Rust value or shared reference.
    unsafe {
        ptr::copy_nonoverlapping(
            source.cast::<u8>(),
            target.cast::<u8>(),
            core::mem::size_of::<CatalogState>(),
        );
        ptr::addr_of_mut!((*target).cached_fonts).write(payload(next));
        ptr::addr_of_mut!((*target).cached_fonts_len).write(MAX_CATALOG_FONTS);
        let entries = ptr::addr_of!((*source).catalog_entries).read();
        if preserve && entries != 0 {
            let length = entries * core::mem::size_of::<CachedFont>();
            let from = ptr::addr_of!((*source).cached_fonts).read();
            ptr::copy_nonoverlapping(from.cast::<u8>(), payload(next).cast::<u8>(), length);
        }
    }
    Some(Replacement {
        bank: next,
        unwind,
        owned: true,
    })
}
pub fn with_catalog<T>(f: impl FnOnce(&CatalogState) -> T) -> T {
    let view = acquire();
    f(unsafe { &*view.state() })
}
pub fn revision() -> u32 {
    with_catalog(|s| s.revision)
}
pub fn reset() {
    if let Some(mut preparation) = begin(true) {
        unsafe { (*preparation.state()).reset() }
        preparation.commit();
    }
}
pub fn reset_catalog() {
    if let Some(mut preparation) = begin(false) {
        preparation.commit();
    }
}
pub fn load_named(path: &[u8], bytes: &[u8]) -> bool {
    let Some(mut preparation) = begin(true) else {
        return false;
    };
    if !unsafe { (*preparation.state()).load_named(path, bytes) } {
        return false;
    }
    preparation.commit();
    true
}
pub fn register(path: &[u8], bytes: &[u8]) -> bool {
    let Some(mut preparation) = begin(true) else {
        return false;
    };
    let renderable = unsafe { (*preparation.state()).register(path, bytes) };
    preparation.commit();
    renderable
}

type CatalogView = View;
type CatalogReplacement = Replacement;

const _: () = {
    assert!(core::mem::size_of::<CatalogView>() == 24);
    assert!(core::mem::align_of::<CatalogView>() == 8);
    assert!(core::mem::offset_of!(CatalogView, bank) == 0);
    assert!(core::mem::offset_of!(CatalogView, unwind) == 8);
};
const _: () = {
    assert!(core::mem::size_of::<CatalogReplacement>() == 32);
    assert!(core::mem::align_of::<CatalogReplacement>() == 8);
    assert!(core::mem::offset_of!(CatalogReplacement, bank) == 0);
    assert!(core::mem::offset_of!(CatalogReplacement, unwind) == 8);
    assert!(core::mem::offset_of!(CatalogReplacement, owned) == 24);
};
