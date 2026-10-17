//! Exact reader/writer admission for one mounted filesystem request lane.
//! Runtime owns only resident metadata; no owner or Rust borrow spans sleep.
use crate::{
    arch::{pit::DEFAULT_HZ, runtime::RuntimeGuard},
    sched::{
        scheduler, sync_objects,
        sync_types::{Text, WaitQueue},
        task,
        task::task_types::WaitResult,
    },
};
use core::{cell::UnsafeCell, ffi::c_void, ptr};
const LANES: usize = 26;
const READERS: usize = 16;
pub(super) const READ: u8 = 1;
pub(super) const WRITE: u8 = 2;
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Owner {
    pub id: u32,
    pub generation: u64,
}
impl Owner {
    const EMPTY: Self = Self {
        id: 0,
        generation: 0,
    };
    fn valid(self) -> bool {
        self.id != 0 && self.generation != 0
    }
}
#[derive(Clone, Copy)]
struct Reader {
    owner: Owner,
    depth: u16,
    borrowed: u16,
}
impl Reader {
    const EMPTY: Self = Self {
        owner: Owner::EMPTY,
        depth: 0,
        borrowed: 0,
    };
}
struct Lane {
    readers: [Reader; READERS],
    writer: Owner,
    writer_depth: u16,
    pending: Owner,
    sequence: u64,
    readers_wait: WaitQueue,
    writers_wait: WaitQueue,
}
impl Lane {
    const fn new() -> Self {
        Self {
            readers: [Reader::EMPTY; READERS],
            writer: Owner::EMPTY,
            writer_depth: 0,
            pending: Owner::EMPTY,
            sequence: 0,
            readers_wait: WaitQueue::EMPTY,
            writers_wait: WaitQueue::EMPTY,
        }
    }
    fn owned_reader(&self, owner: Owner) -> Option<usize> {
        self.readers
            .iter()
            .position(|r| r.depth != 0 && r.owner == owner)
    }
    fn vacant(&self) -> Option<usize> {
        self.readers.iter().position(|r| r.depth == 0)
    }
    fn readers_idle(&self) -> bool {
        self.readers.iter().all(|r| r.depth == 0)
    }
    fn can_read(&self, owner: Owner) -> bool {
        if let Some(i) = self.owned_reader(owner) {
            return self.readers[i].depth != u16::MAX;
        }
        self.vacant().is_some()
            && (self.writer_depth != 0 && self.writer == owner
                || self.writer_depth == 0 && !self.pending.valid())
    }
    fn can_write(&self, owner: Owner) -> bool {
        if self.writer_depth != 0 {
            return self.writer == owner && self.writer_depth != u16::MAX;
        }
        self.readers_idle() && self.pending == owner
    }
}
struct State(UnsafeCell<[Lane; LANES]>);
// Stable boot-lifetime queues; all state and predicates use Runtime admission.
unsafe impl Sync for State {}
static STATE: State = State(UnsafeCell::new([const { Lane::new() }; LANES]));
unsafe fn lane(index: u8) -> *mut Lane {
    assert!(usize::from(index) < LANES);
    unsafe { STATE.0.get().cast::<Lane>().add(usize::from(index)) }
}
/// Nested requests must keep the original non-waiting admission rule even
/// while an outer reader has temporarily returned its backend.
pub(super) fn owns_any(owner: Owner) -> bool {
    let _runtime = RuntimeGuard::acquire();
    (0..LANES).any(|i| unsafe {
        let s = &*lane(i as u8);
        s.writer_depth != 0 && s.writer == owner || s.owned_reader(owner).is_some()
    })
}
pub(super) fn current() -> Owner {
    let _runtime = RuntimeGuard::acquire();
    let task = scheduler::current();
    if task.is_null() {
        return Owner::EMPTY;
    }
    let (id, generation) = unsafe { task::immutable_identity(task) };
    Owner { id, generation }
}
unsafe fn changed(state: *mut Lane) {
    unsafe {
        (*state).sequence = (*state).sequence.wrapping_add(1);
    }
}
unsafe fn wake_readers(state: *mut Lane, count: usize) {
    for _ in 0..count {
        if !unsafe { sync_objects::queue_wake_one(ptr::addr_of_mut!((*state).readers_wait)) } {
            break;
        }
    }
}
#[derive(Clone, Copy)]
struct Wait {
    index: u8,
    mode: u8,
    owner: Owner,
}
// queue_wait_unless calls this only under the same Runtime enrollment owner.
unsafe extern "C" fn needed(raw: *mut c_void) -> bool {
    let c = unsafe { *raw.cast::<Wait>() };
    let state = unsafe { &*lane(c.index) };
    !if c.mode == READ {
        state.can_read(c.owner)
    } else {
        state.can_write(c.owner)
    }
}
fn cancel_pending(index: u8, owner: Owner) {
    let _runtime = RuntimeGuard::acquire();
    let state = unsafe { lane(index) };
    if unsafe { (*state).pending } == owner {
        unsafe {
            (*state).pending = Owner::EMPTY;
            changed(state);
            if (*state).writer_depth == 0 {
                wake_readers(state, READERS);
            }
        }
    }
}
pub(super) fn acquire(index: u8, mode: u8, owner: Owner, immediate: bool) -> bool {
    assert!(mode == READ || mode == WRITE);
    if !owner.valid() || current() != owner {
        return false;
    }
    let mut context = Wait { index, mode, owner };
    let mut sequence = 0;
    let mut stalled = 0;
    let mut contended = false;
    loop {
        let observed = {
            let _runtime = RuntimeGuard::acquire();
            let state = unsafe { lane(index) };
            unsafe {
                if mode == READ {
                    if (*state)
                        .owned_reader(owner)
                        .is_some_and(|i| (*state).readers[i].depth == u16::MAX)
                    {
                        return false;
                    }
                    if (*state).can_read(owner) {
                        let i = (*state)
                            .owned_reader(owner)
                            .or_else(|| (*state).vacant())
                            .unwrap();
                        (*state).readers[i].owner = owner;
                        (*state).readers[i].depth += 1;
                        changed(state);
                        return true;
                    }
                } else {
                    if (*state).writer == owner && (*state).writer_depth == u16::MAX {
                        return false;
                    }
                    // A caller cannot promote its own live reader to writer.
                    // Existing exact writer recursion remains permitted.
                    if (*state).writer != owner && (*state).owned_reader(owner).is_some() {
                        return false;
                    }
                    if !(*state).pending.valid() && (*state).writer_depth == 0 {
                        (*state).pending = owner;
                        changed(state);
                    }
                    if (*state).can_write(owner) {
                        (*state).writer = owner;
                        (*state).writer_depth += 1;
                        (*state).pending = Owner::EMPTY;
                        changed(state);
                        return true;
                    }
                }
                (*state).sequence
            }
        };
        if immediate {
            if mode == WRITE {
                cancel_pending(index, owner);
            }
            return false;
        }
        if !contended {
            super::STATS
                .lock_contention_waits
                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            contended = true;
        }
        if observed != sequence {
            sequence = observed;
            stalled = 0;
        } else {
            stalled += 1;
        }
        if stalled >= 12 {
            super::STATS
                .lock_timeouts
                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            if mode == WRITE {
                cancel_pending(index, owner);
            }
            return false;
        }
        let queue = unsafe {
            let state = lane(index);
            if mode == READ {
                ptr::addr_of_mut!((*state).readers_wait)
            } else {
                ptr::addr_of_mut!((*state).writers_wait)
            }
        };
        let result = unsafe {
            sync_objects::queue_wait_unless(
                queue,
                5 * DEFAULT_HZ as u64,
                Text::literal(if mode == READ {
                    b"fs-read-admit"
                } else {
                    b"fs-write-drain"
                }),
                needed,
                ptr::addr_of_mut!(context).cast(),
            )
        };
        if matches!(result, WaitResult::Failed | WaitResult::Cancelled) {
            if mode == WRITE {
                cancel_pending(index, owner);
            }
            return false;
        }
    }
}
pub(super) fn release(index: u8, mode: u8, owner: Owner) -> bool {
    let _runtime = RuntimeGuard::acquire();
    if current() != owner {
        return false;
    }
    let state = unsafe { lane(index) };
    unsafe {
        if mode == READ {
            let Some(i) = (*state).owned_reader(owner) else {
                return false;
            };
            if (*state).readers[i].borrowed >= (*state).readers[i].depth {
                return false;
            }
            (*state).readers[i].depth -= 1;
            if (*state).readers[i].depth == 0 {
                (*state).readers[i] = Reader::EMPTY;
                if (*state).pending.valid() {
                    if (*state).readers_idle() {
                        sync_objects::queue_wake_one(ptr::addr_of_mut!((*state).writers_wait));
                    }
                } else if (*state).writer_depth == 0 {
                    wake_readers(state, 1);
                }
            }
        } else if mode == WRITE {
            if (*state).writer != owner || (*state).writer_depth == 0 {
                return false;
            }
            if (*state).writer_depth == 1 && !(*state).readers_idle() {
                return false;
            }
            (*state).writer_depth -= 1;
            if (*state).writer_depth == 0 {
                (*state).writer = Owner::EMPTY;
                if (*state).pending.valid() {
                    sync_objects::queue_wake_one(ptr::addr_of_mut!((*state).writers_wait));
                } else {
                    wake_readers(state, READERS);
                }
            }
        } else {
            return false;
        }
        changed(state);
    }
    true
}
/// Exact payload borrow stays inside its admitted native read and mount use.
pub(super) fn borrow(index: u8, owner: Owner, entering: bool) -> bool {
    let _runtime = RuntimeGuard::acquire();
    if current() != owner {
        return false;
    }
    let state = unsafe { lane(index) };
    unsafe {
        let Some(i) = (*state).owned_reader(owner) else {
            return false;
        };
        let r = &mut (*state).readers[i];
        if entering {
            if r.borrowed >= r.depth {
                return false;
            }
            r.borrowed += 1;
        } else {
            if r.borrowed == 0 {
                return false;
            }
            r.borrowed -= 1;
        }
    }
    true
}
/// Only the original boot reset may discard lane metadata/queues.
pub(super) unsafe fn reset() {
    for i in 0..LANES {
        unsafe {
            lane(i as u8).write(Lane::new());
        }
    }
}
