//! Original bounded advisory directory history. Cursors own no resources;
//! mount generation and filesystem node preserve exact directory identity.
use crate::contract::DirectoryChangeCursor as Cursor;
use crate::memory::owner::Guard;
use crate::storage::access_runtime::r4os_rust_storage_access_runtime;
use crate::storage::access_state::{Input, Mount, Reference};
use core::{cell::UnsafeCell, mem::MaybeUninit};

const CAPACITY: usize = 256;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Entry {
    mount: Reference,
    node: u64,
}
#[repr(C)]
pub struct History {
    sequence: u64,
    entries: [MaybeUninit<Entry>; CAPACITY],
}
impl History {
    fn publish(&mut self, mount: Reference, node: u64) {
        self.entries[(self.sequence % CAPACITY as u64) as usize].write(Entry { mount, node });
        self.sequence = self.sequence.wrapping_add(1);
    }
    fn begin(&self, mount: Reference, node: u64) -> Cursor {
        Cursor {
            sequence: self.sequence,
            node,
            mount_slot: mount.slot,
            mount_generation: mount.generation,
            ..Cursor::default()
        }
    }
    // SAFETY: A cursor comes from begin on this same history, or the history
    // entries have all been initialized. The ring is serialized by its owner.
    unsafe fn poll(&self, cursor: &mut Cursor) -> bool {
        let distance = self.sequence.wrapping_sub(cursor.sequence);
        let mut sequence = cursor.sequence;
        cursor.sequence = self.sequence;
        if distance > CAPACITY as u64 {
            return true;
        }
        while sequence != self.sequence {
            let entry =
                unsafe { self.entries[(sequence % CAPACITY as u64) as usize].assume_init_read() };
            if entry.mount.slot == cursor.mount_slot
                && entry.mount.generation == cursor.mount_generation
                && entry.node == cursor.node
            {
                return true;
            }
            sequence = sequence.wrapping_add(1);
        }
        false
    }
}
struct Canonical(UnsafeCell<History>);
// SAFETY: The existing storage metadata owner serializes all accesses. No
// reference into history survives release, callbacks or scheduler wakes.
unsafe impl Sync for Canonical {}
// Initialize the resident ring as well: public cursors are caller-writable,
// including a forged wrapped sequence before the first 256 publications.
// Valid begin/poll behavior remains identical; no uninitialized Rust read.
static HISTORY: Canonical = Canonical(UnsafeCell::new(History {
    sequence: 0,
    entries: [MaybeUninit::new(Entry {
        mount: Reference {
            slot: 0,
            generation: 0,
        },
        node: 0,
    }); CAPACITY],
}));

/// # Safety
/// State is aligned resident storage serialized by its caller. Publish input
/// is initialized; begin output is exclusive and disjoint from history. Poll
/// uses a valid cursor from this history (or a fully initialized ring), and
/// cursor storage is exclusive and disjoint. No callback or wait occurs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fs_directory_history(
    history: *mut History,
    operation: u8,
    mount: *const Reference,
    node: u64,
    cursor: *mut Cursor,
) -> bool {
    match operation {
        0 => unsafe { (&mut *history).publish(mount.read(), node) },
        1 => unsafe { cursor.write((&*history).begin(mount.read(), node)) },
        2 => return unsafe { (&*history).poll(&mut *cursor) },
        _ => unreachable!(),
    }
    false
}

/// # Safety
/// Original task-context entry. Mount input is an initialized resident value;
/// begin/poll cursor is exclusive caller storage and must remain resident
/// through the bounded call. Only notification calls the desktop wake after
/// releasing the storage owner. No borrow of history or cursor spans a wake.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fs_directory_changes(
    operation: u8,
    mount: *const Reference,
    node: u64,
    cursor: *mut Cursor,
) -> i32 {
    match operation {
        0 => {
            {
                let _guard = Guard::acquire(1);
                unsafe { (&mut *HISTORY.0.get()).publish(mount.read(), node) };
            }
            unsafe { fs_directory_activity() };
            0
        }
        1 => {
            let _guard = Guard::acquire(1);
            unsafe { cursor.write((&*HISTORY.0.get()).begin(mount.read(), node)) };
            0
        }
        2 => {
            let value = unsafe { cursor.read() };
            if value.version != 1
                || value.size != 40
                || value.reserved != 0
                || value.mount_generation == 0
            {
                return -1;
            }
            let input = Input {
                reference: Reference {
                    slot: value.mount_slot,
                    generation: value.mount_generation,
                },
                ..Input::default()
            };
            let mut scalar = 0;
            let mut snapshot = MaybeUninit::<Mount>::uninit();
            // Mount validation and the history snapshot are one RAM-only
            // metadata transaction. Unmount/remount cannot slip between them.
            let _guard = Guard::acquire(1);
            let status = unsafe {
                r4os_rust_storage_access_runtime(
                    8,
                    true,
                    &input,
                    &mut scalar,
                    snapshot.as_mut_ptr().cast(),
                )
            };
            if status != 0 {
                return -3;
            }
            i32::from(unsafe { (&*HISTORY.0.get()).poll(&mut *cursor) })
        }
        _ => unreachable!(),
    }
}

/// Capture both namespace watches at one publication sequence. The caller
/// retains its original request while resolving these nodes.
pub(crate) fn begin_scan(mount: Reference, node: u64, parent: u64) -> (Cursor, Cursor) {
    let _guard = Guard::acquire(1);
    let history = unsafe { &*HISTORY.0.get() };
    (history.begin(mount, node), history.begin(mount, parent))
}
/// Validate the exact mounted generation and both watches in one RAM-only
/// transaction. Global sequence changes still reach the caller even when the
/// mutation belongs to another directory and neither watch reports a hit.
pub(crate) fn poll_scan(cursor: &mut Cursor, parent: &mut Cursor) -> i32 {
    if cursor.version != 1
        || cursor.size != 40
        || cursor.reserved != 0
        || cursor.mount_generation == 0
        || parent.version != 1
        || parent.size != 40
        || parent.reserved != 0
        || parent.mount_slot != cursor.mount_slot
        || parent.mount_generation != cursor.mount_generation
        || parent.sequence != cursor.sequence
    {
        return -1;
    }
    let _guard = Guard::acquire(1);
    let input = Input {
        reference: Reference {
            slot: cursor.mount_slot,
            generation: cursor.mount_generation,
        },
        ..Input::default()
    };
    let mut scalar = 0;
    let mut snapshot = MaybeUninit::<Mount>::uninit();
    if unsafe {
        r4os_rust_storage_access_runtime(8, true, &input, &mut scalar, snapshot.as_mut_ptr().cast())
    } != 0
    {
        return -3;
    }
    let history = unsafe { &*HISTORY.0.get() };
    // Always consume both at the same sequence, including a first-watch hit.
    let changed = unsafe { history.poll(cursor) };
    let parent_changed = unsafe { history.poll(parent) };
    i32::from(changed || parent_changed)
}

const _: () = {
    assert!(core::mem::size_of::<Entry>() == 24);
    assert!(core::mem::offset_of!(History, entries) == 8);
    assert!(core::mem::size_of::<History>() == 6152);
    assert!(core::mem::size_of::<Cursor>() == 40);
};

unsafe fn fs_directory_activity() {
    crate::program::gui_activity::notify(crate::contract::gui_activity_shared_directory);
}
