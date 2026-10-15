//! Single resident registry storage. Legacy domain peers temporarily borrow
//! these exact records under the original registry/boot ownership rules;
//! they retain no second head, counter, generation, queue, event or history.
use super::registry_types::*;
use crate::sched::sync_types::{Event, Mutex, Text, WaitQueue};
use core::{cell::UnsafeCell, ptr};

struct Cell<T>(UnsafeCell<T>);
// All access follows the existing registry mutex, exact task/instance pins,
// and single-threaded boot boundary. No mutable Rust reference spans peers.
unsafe impl<T> Sync for Cell<T> {}
impl<T> Cell<T> {
    const fn new(value: T) -> Self {
        Self(UnsafeCell::new(value))
    }
    const fn get(&self) -> *mut T {
        self.0.get()
    }
}

#[repr(C)]
pub struct Shared {
    pub head: *mut ProgramRegistryChunk,
    pub tail: *mut ProgramRegistryChunk,
    pub stats: *mut ProgramRegistryStats,
    pub mutex: *mut Mutex,
    pub event: *mut Event,
    pub history: *mut [ExitRecord; 16],
    pub epoch: u64,
    pub next_chunk_serial: u64,
    pub next_generation: u64,
    pub history_head: usize,
    pub history_count: usize,
    pub next_exit_sequence: u64,
    pub last_exit_sequence: u64,
    pub completion_head: *mut ProgramCompletionNode,
    pub completion_tail: *mut ProgramCompletionNode,
    pub retire_head: *mut ProgramRegistrySlot,
    pub retire_tail: *mut ProgramRegistrySlot,
    pub failure_cursor: u32,
    pub next_instance_id: u32,
    pub failure_after: u32,
    pub failure_after_present: bool,
    pub next_growth_failure: bool,
    pub generation_exhausted: bool,
    pub reaper_started: bool,
}
const fn initial_stats() -> ProgramRegistryStats {
    // Integer/bool-only snapshot record, including explicit C padding.
    let mut stats: ProgramRegistryStats = unsafe { core::mem::zeroed() };
    stats.chunk_slots = 16;
    stats.next_generation = 1;
    stats
}
static STATS: Cell<ProgramRegistryStats> = Cell::new(initial_stats());
static MUTEX: Cell<Mutex> = Cell::new(Mutex {
    rank: 110,
    mode: 0,
    name: Text::literal(b"r4x-program-registry"),
    ..Mutex::EMPTY
});
static EVENT: Cell<Event> = Cell::new(Event {
    signaled: false,
    mode: 1,
    queue: WaitQueue::EMPTY,
});
// ExitRecord has only plain C integer/bool fields; the original empty history
// uses zero handles/sequences and natural exit reason zero.
static HISTORY: Cell<[ExitRecord; 16]> = Cell::new(unsafe { core::mem::zeroed() });
static STATE: Cell<Shared> = Cell::new(Shared {
    head: ptr::null_mut(),
    tail: ptr::null_mut(),
    stats: STATS.get(),
    mutex: MUTEX.get(),
    event: EVENT.get(),
    history: HISTORY.get(),
    epoch: 1,
    next_chunk_serial: 1,
    next_generation: 1,
    history_head: 0,
    history_count: 0,
    next_exit_sequence: 1,
    last_exit_sequence: 0,
    completion_head: ptr::null_mut(),
    completion_tail: ptr::null_mut(),
    retire_head: ptr::null_mut(),
    retire_tail: ptr::null_mut(),
    failure_cursor: 0,
    next_instance_id: 1,
    failure_after: 0,
    failure_after_present: false,
    next_growth_failure: false,
    generation_exhausted: false,
    reaper_started: false,
});
pub fn state() -> *mut Shared {
    STATE.get()
}
/// Original temporary C views borrow resident native storage. Every mutation
/// must keep the same registry/runtime/boot admission and exact identity.
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_registry_state() -> *mut Shared {
    state()
}
const _: () = {
    assert!(core::mem::size_of::<Shared>() == 152);
    assert!(core::mem::offset_of!(Shared, epoch) == 48);
    assert!(core::mem::offset_of!(Shared, next_generation) == 64);
    assert!(core::mem::offset_of!(Shared, failure_cursor) == 136);
    assert!(core::mem::offset_of!(Shared, next_instance_id) == 140);
};
