//! Four immutable resident decoded units. Metadata only under owner1; no
//! allocation, free, I/O or potentially faulting caller copy under that owner.
use super::{ntfs_decoded_key::Key, ntfs_format_types::Buffer, ntfs_volume_cache::Cache};
use crate::memory::{heap, owner::Guard};
use core::{cell::UnsafeCell, ptr};
const SLOTS: usize = 4;
const BYTES: usize = 65536;
const EMPTY: u8 = 0;
const READY: u8 = 1;
const LOADING: u8 = 2;
const RETIRING: u8 = 3;
#[derive(Clone, Copy)]
struct Entry {
    state: u8,
    key: Key,
    storage: *mut u8,
    serial: u64,
    pins: u32,
    used: u64,
}
impl Entry {
    const EMPTY: Self = Self {
        state: EMPTY,
        key: Key::EMPTY,
        storage: ptr::null_mut(),
        serial: 0,
        pins: 0,
        used: 0,
    };
}
struct Pool {
    entries: [Entry; SLOTS],
    serial: u64,
    clock: u64,
}
struct State(UnsafeCell<Pool>);
// Resident metadata is accessed only under the existing memory owner1.
unsafe impl Sync for State {}
static STATE: State = State(UnsafeCell::new(Pool {
    entries: [Entry::EMPTY; SLOTS],
    serial: 0,
    clock: 0,
}));
unsafe fn pool() -> *mut Pool {
    STATE.0.get()
}
unsafe fn entry(index: usize) -> *mut Entry {
    assert!(index < SLOTS);
    unsafe {
        ptr::addr_of_mut!((*pool()).entries)
            .cast::<Entry>()
            .add(index)
    }
}
unsafe fn touch() -> u64 {
    unsafe {
        (*pool()).clock = (*pool()).clock.wrapping_add(1);
        (*pool()).clock
    }
}
unsafe fn serial() -> Option<u64> {
    unsafe {
        let next = (*pool()).serial.checked_add(1)?;
        (*pool()).serial = next;
        Some(next)
    }
}
pub(super) struct Pin {
    index: usize,
    serial: u64,
    storage: *mut u8,
    len: usize,
}
impl Pin {
    /// Exact live pin; destination is disjoint caller output. No metadata owner
    /// or mutable table borrow survives this possibly faulting copy.
    pub unsafe fn copy(&self, offset: usize, out: Buffer) {
        assert!(offset <= self.len && out.len <= self.len - offset);
        unsafe {
            ptr::copy_nonoverlapping(self.storage.add(offset), out.data, out.len);
        }
    }
}
impl Drop for Pin {
    fn drop(&mut self) {
        let _owner = Guard::acquire(1);
        unsafe {
            let e = entry(self.index);
            assert!(
                (*e).state == READY
                    && (*e).serial == self.serial
                    && (*e).storage == self.storage
                    && (*e).pins != 0,
                "Decoded pin identity"
            );
            (*e).pins -= 1;
        }
    }
}
pub(super) unsafe fn lookup(key: Key) -> Option<Pin> {
    let _owner = Guard::acquire(1);
    if !unsafe { key.current() } {
        return None;
    }
    for index in 0..SLOTS {
        unsafe {
            let e = entry(index);
            if (*e).state == READY && (*e).key == key && (*e).pins != u32::MAX {
                assert!(!(*e).storage.is_null());
                (*e).pins += 1;
                (*e).used = touch();
                return Some(Pin {
                    index,
                    serial: (*e).serial,
                    storage: (*e).storage,
                    len: key.bytes,
                });
            }
        }
    }
    None
}
struct Fill {
    index: usize,
    serial: u64,
    storage: *mut u8,
    committed: bool,
}
unsafe fn reserve(key: Key) -> Option<Fill> {
    let _owner = Guard::acquire(1);
    if !unsafe { key.current() } {
        return None;
    }
    let mut chosen = None;
    let mut oldest = u64::MAX;
    for index in 0..SLOTS {
        unsafe {
            let e = entry(index);
            if matches!((*e).state, READY | LOADING) && (*e).key == key {
                return None;
            }
            if (*e).state == EMPTY {
                chosen = Some(index);
                oldest = 0;
            } else if oldest != 0 && (*e).state == READY && (*e).pins == 0 && (*e).used < oldest {
                chosen = Some(index);
                oldest = (*e).used;
            }
        }
    }
    let index = chosen?;
    let serial = unsafe { serial() }?;
    let e = unsafe { entry(index) };
    let storage = unsafe { (*e).storage };
    unsafe {
        e.write(Entry {
            state: LOADING,
            key,
            storage,
            serial,
            pins: 0,
            used: touch(),
        });
    }
    Some(Fill {
        index,
        serial,
        storage,
        committed: false,
    })
}
/// The slot keeps its budget and pointer in Retiring until actual heap release.
unsafe fn finish_retire(index: usize, serial: u64, storage: *mut u8) {
    if !storage.is_null() {
        assert_eq!(
            unsafe { heap::r4os_rust_heap_free(storage, BYTES) },
            0,
            "Decoded resident free acknowledgement"
        );
    }
    let _owner = Guard::acquire(1);
    unsafe {
        let e = entry(index);
        assert!(
            (*e).state == RETIRING
                && (*e).serial == serial
                && (*e).storage == storage
                && (*e).pins == 0,
            "Decoded retire identity"
        );
        e.write(Entry::EMPTY);
    }
}
impl Drop for Fill {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        {
            let _owner = Guard::acquire(1);
            unsafe {
                let e = entry(self.index);
                assert!(
                    (*e).state == LOADING
                        && (*e).serial == self.serial
                        && (*e).storage == self.storage,
                    "Decoded abort identity"
                );
                (*e).state = RETIRING;
            }
        }
        unsafe {
            finish_retire(self.index, self.serial, self.storage);
        }
    }
}
/// Original successful decode/zero-fill is resident and complete before entry.
/// Cache admission or allocation failure never changes that read's result.
pub(super) unsafe fn remember(key: Key, decoded: Buffer) {
    assert_eq!(decoded.len, key.bytes);
    let Some(mut fill) = (unsafe { reserve(key) }) else {
        return;
    };
    if fill.storage.is_null() {
        let storage = heap::r4os_rust_heap_alloc(BYTES, 16);
        if storage.is_null() {
            return;
        }
        fill.storage = storage;
        {
            let _owner = Guard::acquire(1);
            unsafe {
                let e = entry(fill.index);
                assert!(
                    (*e).state == LOADING && (*e).serial == fill.serial && (*e).storage.is_null(),
                    "Decoded allocation ticket"
                );
                (*e).storage = storage;
            }
        }
    }
    unsafe {
        ptr::copy_nonoverlapping(decoded.data, fill.storage, decoded.len);
    }
    let _owner = Guard::acquire(1);
    unsafe {
        let e = entry(fill.index);
        assert!(
            (*e).state == LOADING && (*e).serial == fill.serial && (*e).storage == fill.storage,
            "Decoded publication ticket"
        );
        if !key.current() {
            return;
        }
        (*e).state = READY;
        (*e).used = touch();
        fill.committed = true;
    }
}
/// Bounded canonical pressure work or exact mount teardown. Only unpinned
/// Ready data can be retired; reservations and in-flight frees keep their slot.
unsafe fn retire(max: usize, cache: *mut Cache) -> usize {
    let mut freed = 0;
    for _ in 0..max.min(SLOTS) {
        let selected = {
            let _owner = Guard::acquire(1);
            let mut found = None;
            for index in 0..SLOTS {
                unsafe {
                    let e = entry(index);
                    if (*e).state == READY
                        && (*e).pins == 0
                        && (cache.is_null() || (*e).key.cache == cache)
                    {
                        (*e).state = RETIRING;
                        found = Some((index, (*e).serial, (*e).storage));
                        break;
                    }
                }
            }
            found
        };
        let Some((index, serial, storage)) = selected else {
            break;
        };
        unsafe {
            finish_retire(index, serial, storage);
        }
        freed += 1;
    }
    freed
}
pub(crate) unsafe fn reclaim(max: usize) -> usize {
    unsafe { retire(max, ptr::null_mut()) }
}
pub(super) unsafe fn forget(cache: *mut Cache) {
    if !cache.is_null() {
        unsafe {
            retire(SLOTS, cache);
        }
    }
}
