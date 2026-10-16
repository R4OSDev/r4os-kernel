//! Exact stream identities, lane-scoped lookups and nonblocking lifecycle
//! cleanup. The separate reservation projection admits cross-volume scans
//! without reading another lane's mutable stream payload.
use super::{context, fs, path, stream_publish as publish, stream_state as s};
use crate::{memory::owner::Guard, sched::scheduler};
use core::ptr;
pub(super) unsafe fn owned(slot: *const s::Slot) -> bool {
    unsafe {
        if let Some(owner) = context::owner() {
            if (*slot).owner_kind != owner.kind
                || (*slot).owner_id != owner.id
                || (*slot).owner_generation != owner.generation
            {
                return false;
            }
            return owner.kind != 0
                || ((*slot).owner_program_id == owner.program_id
                    && (*slot).owner_program_generation == owner.program_generation);
        }
        (*slot).owner_id == 0 && (*slot).owner_generation == 0
    }
}
unsafe fn alive(slot: *const s::Slot) -> bool {
    unsafe {
        if (*slot).owner_id == 0 || (*slot).owner_generation == 0 {
            return !context::has_owner_provider() && scheduler::current().is_null();
        }
        (*slot).owner_kind == 0 || context::alive((*slot).owner_id, (*slot).owner_generation)
    }
}
pub(super) unsafe fn matches(
    slot: *const s::Slot,
    letter: u8,
    boot: bool,
    parent: u64,
    entry: Option<&fs::Entry>,
    name: &[u8],
) -> bool {
    unsafe {
        if !(*slot).active || (*slot).drive_letter != letter || (*slot).on_boot_volume != boot {
            return false;
        }
        if let Some(entry) = entry {
            if !(*slot).has_file_node {
                return false;
            }
            if (*slot).file_node_generation != 0 && entry.generation != 0 {
                return (*slot).file_node == entry.node
                    && (*slot).file_node_generation == entry.generation;
            }
            if (*slot).parent_node != parent {
                return false;
            }
            let same_name = s::name(slot).eq_ignore_ascii_case(&entry.name[..entry.name_len]);
            if (*slot).identity_refresh_pending
                && same_name
                && entry.generation == 0
                && entry.size == (*slot).size
            {
                return true;
            }
            if (*slot).file_node == 0 && (*slot).file_node_generation == 0 && entry.generation == 0
            {
                return same_name && s::fingerprint(slot, entry);
            }
            if (*slot).file_node != 0
                || (*slot).file_node_generation != 0
                || entry.node != 0
                || entry.generation != 0
            {
                return (*slot).file_node == entry.node
                    && (*slot).file_node_generation == entry.generation;
            }
            return same_name;
        }
        (*slot).parent_node == parent && s::name(slot).eq_ignore_ascii_case(name)
    }
}
pub(super) unsafe fn find(
    letter: u8,
    boot: bool,
    parent: u64,
    entry: Option<&fs::Entry>,
    name: &[u8],
    owner_only: bool,
) -> Option<*mut s::Slot> {
    unsafe {
        for index in 0..16 {
            if !s::lane_owned(index, letter) {
                continue;
            }
            let slot = s::slot(index);
            if owner_only && !owned(slot) {
                continue;
            }
            if matches(slot, letter, boot, parent, entry, name) {
                return Some(slot);
            }
        }
        None
    }
}
pub(super) unsafe fn reap(letter: u8) {
    unsafe {
        // A volume request never authorizes reads/cleanup in a foreign lane.
        // Dead foreign slots remain for their own next gate entry or exact retire.
        for index in 0..16 {
            if !s::lane_owned(index, letter) {
                continue;
            }
            let slot = s::slot(index);
            if !(*slot).active || alive(slot) {
                continue;
            }
            if (*slot).publish_started {
                publish::settle(slot);
                continue;
            }
            s::clear(slot);
        }
    }
}
unsafe fn publish_matches(
    slot: *const s::Slot,
    letter: u8,
    boot: bool,
    parent: u64,
    entry: Option<&fs::Entry>,
    name: &[u8],
) -> bool {
    unsafe {
        if !(*slot).active
            || !(*slot).publish_started
            || (*slot).drive_letter != letter
            || (*slot).on_boot_volume != boot
        {
            return false;
        }
        if entry.is_some_and(|entry| s::owns_entry(slot, entry)) {
            return true;
        }
        if (*slot).parent_node != parent {
            return false;
        }
        let candidate = entry.map_or(name, |entry| &entry.name[..entry.name_len]);
        path::could_alias(s::name(slot), candidate)
            || path::could_alias(publish::target(slot), candidate)
            || path::could_alias(publish::backup(slot), candidate)
    }
}
pub(super) unsafe fn collision(
    letter: u8,
    boot: bool,
    parent: u64,
    entry: Option<&fs::Entry>,
    name: &[u8],
    ignored: *const s::Slot,
) -> bool {
    unsafe {
        for index in 0..16 {
            if !s::lane_owned(index, letter) {
                continue;
            }
            let slot = s::slot(index);
            if ptr::eq(slot, ignored) {
                continue;
            }
            if publish_matches(slot, letter, boot, parent, entry, name) {
                return true;
            }
        }
        false
    }
}
pub(super) unsafe fn invalidate(
    letter: u8,
    boot: bool,
    parent: u64,
    entry: Option<&fs::Entry>,
    name: &[u8],
) -> bool {
    unsafe {
        if collision(letter, boot, parent, entry, name, ptr::null()) {
            return false;
        }
        for index in 0..16 {
            if !s::lane_owned(index, letter) {
                continue;
            }
            let slot = s::slot(index);
            if !(*slot).active
                || (*slot).drive_letter != letter
                || (*slot).on_boot_volume != boot
                || (*slot).publish_started
            {
                continue;
            }
            if let Some(entry) = entry
                && (*slot).has_file_node
                && (*slot).file_node_generation != 0
                && entry.generation != 0
                && (*slot).file_node == entry.node
                && (*slot).file_node_generation == entry.generation
            {
                s::clear(slot);
                continue;
            }
            if (*slot).parent_node != parent {
                continue;
            }
            let matched = entry.map_or_else(
                || s::name(slot).eq_ignore_ascii_case(name),
                |entry| {
                    ((*slot).has_file_node
                        && (*slot).file_node != 0
                        && entry.node != 0
                        && (*slot).file_node == entry.node)
                        || s::name(slot).eq_ignore_ascii_case(&entry.name[..entry.name_len])
                },
            );
            if matched {
                s::clear(slot);
            }
        }
        true
    }
}
pub(super) unsafe fn subtree(letter: u8, boot: bool, directory: &[u8], entry: &fs::Entry) -> bool {
    unsafe {
        reap(letter);
        for index in 0..16 {
            if !s::lane_owned(index, letter) {
                continue;
            }
            let slot = s::slot(index);
            if !(*slot).active || (*slot).drive_letter != letter || (*slot).on_boot_volume != boot {
                continue;
            }
            if entry.node != 0 {
                for index in 0..(*slot).ancestor_count {
                    if (*slot).ancestor_nodes[index] == entry.node
                        && (*slot).ancestor_generations[index] == entry.generation
                    {
                        return true;
                    }
                }
            }
            let raw = core::slice::from_raw_parts(
                ptr::addr_of!((*slot).raw_path).cast(),
                (*slot).raw_len,
            );
            if path::descendant(raw, directory) {
                return true;
            }
        }
        false
    }
}
#[derive(Clone, Copy)]
struct Cleanup {
    program: u32,
    program_generation: u64,
    task: u32,
    task_generation: u64,
    scoped: bool,
}
fn owner_matches(owner: s::Projection, cleanup: Cleanup) -> bool {
    owner.reserved
        && owner.owner_kind == 0
        && owner.owner_program_id == cleanup.program
        && owner.owner_program_generation == cleanup.program_generation
        && (!cleanup.scoped
            || owner.owner_id == cleanup.task && owner.owner_generation == cleanup.task_generation)
}
unsafe fn slot_matches(slot: *const s::Slot, cleanup: Cleanup) -> bool {
    unsafe {
        (*slot).active
            && (*slot).owner_kind == 0
            && (*slot).owner_program_id == cleanup.program
            && (*slot).owner_program_generation == cleanup.program_generation
            && (!cleanup.scoped
                || (*slot).owner_id == cleanup.task
                    && (*slot).owner_generation == cleanup.task_generation)
    }
}
fn lane(letter: u8) -> Option<u8> {
    let letter = letter.to_ascii_uppercase();
    if letter.is_ascii_uppercase() {
        Some(letter - b'A')
    } else {
        None
    }
}
struct Plan {
    lanes: [u16; 26],
    invalid: bool,
}
impl Plan {
    fn empty(&self) -> bool {
        self.lanes.iter().all(|&mask| mask == 0)
    }
}
unsafe fn plan(cleanup: Cleanup) -> Plan {
    unsafe {
        let _guard = Guard::acquire(0);
        let mut plan = Plan {
            lanes: [0; 26],
            invalid: false,
        };
        for index in 0..16 {
            let owner = s::projection_locked(index);
            if !owner_matches(owner, cleanup) {
                continue;
            }
            if let Some(lane) = lane(owner.drive_letter) {
                plan.lanes[usize::from(lane)] |= 1u16 << index;
            } else {
                plan.invalid = true;
            }
        }
        plan
    }
}
unsafe fn still_matches(index: usize, expected: u8, cleanup: Cleanup) -> bool {
    unsafe {
        if index >= 16 {
            return false;
        }
        let _guard = Guard::acquire(0);
        let owner = s::projection_locked(index);
        owner_matches(owner, cleanup) && lane(owner.drive_letter) == Some(expected)
    }
}
unsafe fn release(cleanup: Cleanup) -> bool {
    unsafe {
        let plan = plan(cleanup);
        if plan.invalid {
            return false;
        }
        if plan.empty() {
            return true;
        }
        for lane in 0..26u8 {
            let mask = plan.lanes[usize::from(lane)];
            if mask == 0 {
                continue;
            }
            let Some(mut request) = fs::Request::begin(9, b'A' + lane, b'C', 1, true, &[]) else {
                return false;
            };
            let mut released = true;
            for index in 0..16 {
                if mask & (1u16 << index) == 0 || !still_matches(index, lane, cleanup) {
                    continue;
                }
                let slot = s::slot(index);
                if !slot_matches(slot, cleanup) {
                    released = false;
                    continue;
                }
                if (*slot).publish_started && !publish::settle(slot) {
                    released = false;
                    continue;
                }
                if (*slot).active {
                    s::clear(slot);
                }
            }
            request.ok = released;
            drop(request);
            if !released {
                return false;
            }
        }
        let remaining = self::plan(cleanup);
        !remaining.invalid && remaining.empty()
    }
}
/// # Safety
/// Exact process/thread generation has drained all caller-bound async I/O.
/// This cleanup never parks for admission or touches an unrelated stream payload. The same immediate ordered
/// admission also protects the C: catalog used by exact claim retirement.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_stream_cleanup(
    program: u32,
    generation: u64,
    task: u32,
    task_generation: u64,
    scoped: bool,
) -> bool {
    unsafe {
        if program == 0 || generation == 0 || scoped && (task == 0 || task_generation == 0) {
            return true;
        }
        release(Cleanup {
            program,
            program_generation: generation,
            task,
            task_generation,
            scoped,
        })
    }
}
