//! Native stream payloads and separate allocation/owner projection. Payloads
//! belong to their exact filesystem lane; only projection/generation admission
//! is shared across lanes. Slot addresses remain stable until exact cleanup.
use super::{context::StreamOwner, fs, path};
use crate::{
    memory::owner::Guard,
    storage::{
        access_runtime,
        access_state::{Input, Owner, Reference},
    },
};
use core::{
    cell::UnsafeCell,
    ptr,
    sync::atomic::{AtomicU32, Ordering},
};
#[derive(Clone, Copy)]
pub(super) struct Use {
    pub id: u64,
    pub owner: Owner,
}
pub(super) struct Slot {
    pub volume_use: Option<Use>,
    pub active: bool,
    pub ready: bool,
    pub finished: bool,
    pub publish_started: bool,
    pub publish_target_name: [u8; 768],
    pub publish_target_name_len: usize,
    pub publish_backup_name: [u8; 12],
    pub publish_backup_name_len: usize,
    pub publish_claim_generation: u64,
    pub publish_declared: bool,
    pub publish_declared_parent: [u8; 1024],
    pub publish_declared_parent_len: usize,
    pub publish_protocol: u8,
    pub raw_path: [u8; 1024],
    pub raw_len: usize,
    pub drive_letter: u8,
    pub on_boot_volume: bool,
    pub parent_node: u64,
    pub ancestor_nodes: [u64; 260],
    pub ancestor_generations: [u16; 260],
    pub ancestor_count: usize,
    pub file_node: u64,
    pub file_node_generation: u16,
    pub has_file_node: bool,
    pub identity_refresh_pending: bool,
    pub file_attr: u8,
    pub file_reparse: bool,
    pub file_created_time: u16,
    pub file_created_date: u16,
    pub file_access_date: u16,
    pub file_modified_time: u16,
    pub file_modified_date: u16,
    pub name: [u8; 1024],
    pub name_len: usize,
    pub size: u64,
    pub flags: u32,
    pub generation: u32,
    pub owner_kind: u8,
    pub owner_id: u32,
    pub owner_generation: u64,
    pub owner_program_id: u32,
    pub owner_program_generation: u64,
}
impl Slot {
    pub const EMPTY: Self = Self {
        volume_use: None,
        active: false,
        ready: false,
        finished: false,
        publish_started: false,
        publish_target_name: [0; 768],
        publish_target_name_len: 0,
        publish_backup_name: [0; 12],
        publish_backup_name_len: 0,
        publish_claim_generation: 0,
        publish_declared: false,
        publish_declared_parent: [0; 1024],
        publish_declared_parent_len: 0,
        publish_protocol: 0,
        raw_path: [0; 1024],
        raw_len: 0,
        drive_letter: 0,
        on_boot_volume: false,
        parent_node: 0,
        ancestor_nodes: [0; 260],
        ancestor_generations: [0; 260],
        ancestor_count: 0,
        file_node: 0,
        file_node_generation: 0,
        has_file_node: false,
        identity_refresh_pending: false,
        file_attr: 0,
        file_reparse: false,
        file_created_time: 0,
        file_created_date: 0,
        file_access_date: 0,
        file_modified_time: 0,
        file_modified_date: 0,
        name: [0; 1024],
        name_len: 0,
        size: 0,
        flags: 0,
        generation: 0,
        owner_kind: 1,
        owner_id: 0,
        owner_generation: 0,
        owner_program_id: 0,
        owner_program_generation: 0,
    };
}
#[derive(Clone, Copy)]
pub(super) struct Projection {
    pub reserved: bool,
    pub drive_letter: u8,
    pub owner_kind: u8,
    pub owner_id: u32,
    pub owner_generation: u64,
    pub owner_program_id: u32,
    pub owner_program_generation: u64,
}
impl Projection {
    pub const EMPTY: Self = Self {
        reserved: false,
        drive_letter: 0,
        owner_kind: 1,
        owner_id: 0,
        owner_generation: 0,
        owner_program_id: 0,
        owner_program_generation: 0,
    };
}
// Keep room for one stream per supported CPU plus concurrent system/export
// streams. Payload/projection bounds and retirement masks share this limit.
pub(super) const CAPACITY: usize = crate::arch::smp_policy::MAX_CPUS * 2;
const _: () = assert!(CAPACITY <= u128::BITS as usize);
struct Slots(UnsafeCell<[Slot; CAPACITY]>);
// SAFETY: Per-drive request gates serialize payload access; the program-state
// owner serializes the separate reservation projection. A reader must filter
// the projection under that owner before touching a lane's mutable payload.
unsafe impl Sync for Slots {}
static SLOTS: Slots = Slots(UnsafeCell::new([const { Slot::EMPTY }; CAPACITY]));
struct ProjectionTable(UnsafeCell<[Projection; CAPACITY]>);
// SAFETY: All projection access is under the existing program-state owner.
unsafe impl Sync for ProjectionTable {}
static PROJECTIONS: ProjectionTable =
    ProjectionTable(UnsafeCell::new([Projection::EMPTY; CAPACITY]));
static GENERATION: AtomicU32 = AtomicU32::new(1);
pub(super) fn slot(index: usize) -> *mut Slot {
    assert!(index < CAPACITY);
    unsafe { SLOTS.0.get().cast::<Slot>().add(index) }
}
pub(super) fn lane_owned(index: usize, letter: u8) -> bool {
    assert!(index < CAPACITY);
    let _guard = Guard::acquire(0);
    let owner = unsafe { PROJECTIONS.0.get().cast::<Projection>().add(index).read() };
    owner.reserved && owner.drive_letter == letter
}
pub(super) unsafe fn reserve(letter: u8, owner: Option<StreamOwner>) -> Option<*mut Slot> {
    unsafe {
        let _guard = Guard::acquire(0);
        for index in 0..CAPACITY {
            let projection = PROJECTIONS.0.get().cast::<Projection>().add(index);
            if (*projection).reserved {
                continue;
            }
            let owner = owner.unwrap_or(StreamOwner {
                kind: 1,
                ..Default::default()
            });
            projection.write(Projection {
                reserved: true,
                drive_letter: letter,
                owner_kind: owner.kind,
                owner_id: owner.id,
                owner_generation: owner.generation,
                owner_program_id: owner.program_id,
                owner_program_generation: owner.program_generation,
            });
            return Some(slot(index));
        }
        None
    }
}
pub(super) unsafe fn release(slot: *mut Slot) {
    unsafe {
        let _guard = Guard::acquire(0);
        for index in 0..CAPACITY {
            if self::slot(index) == slot {
                PROJECTIONS
                    .0
                    .get()
                    .cast::<Projection>()
                    .add(index)
                    .write(Projection::EMPTY);
                return;
            }
        }
    }
}
pub(super) fn generation() -> u32 {
    let mut next = GENERATION.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
    if next == 0 {
        next = GENERATION.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
    }
    next
}
pub(super) unsafe fn begin_use(reference: Reference) -> Result<Use, i32> {
    unsafe {
        let mut owner = Owner::default();
        if !access_runtime::r4os_rust_storage_access_current(&mut owner) {
            return Err(-1);
        }
        let mut id = 0;
        let result = access_runtime::r4os_rust_storage_access_runtime(
            10,
            false,
            &Input {
                reference,
                owner,
                kind: 1,
                ..Default::default()
            },
            &mut id,
            ptr::null_mut(),
        );
        if result == 0 {
            Ok(Use { id, owner })
        } else {
            Err(result)
        }
    }
}
pub(super) unsafe fn end_use(token: Use) {
    unsafe {
        let mut ignored = 0;
        access_runtime::r4os_rust_storage_access_runtime(
            12,
            false,
            &Input {
                id: token.id,
                owner: token.owner,
                ..Default::default()
            },
            &mut ignored,
            ptr::null_mut(),
        );
    }
}
// Preserve the original one-step slot publication operands without another owner record.
#[allow(clippy::too_many_arguments)]
pub(super) unsafe fn remember(
    slot: *mut Slot,
    raw: &[u8],
    letter: u8,
    boot: bool,
    parent: u64,
    name: &[u8],
    flags: u32,
    owner: Option<StreamOwner>,
) {
    unsafe {
        if raw.len() > 1024 || name.len() > 1024 {
            return;
        }
        slot.write(Slot::EMPTY);
        let owner = owner.unwrap_or(StreamOwner {
            kind: 1,
            ..Default::default()
        });
        (*slot).active = true;
        (*slot).raw_len = raw.len();
        (*slot).drive_letter = letter;
        (*slot).on_boot_volume = boot;
        (*slot).parent_node = parent;
        (*slot).name_len = name.len();
        (*slot).flags = flags;
        (*slot).generation = generation();
        (*slot).owner_kind = owner.kind;
        (*slot).owner_id = owner.id;
        (*slot).owner_generation = owner.generation;
        (*slot).owner_program_id = owner.program_id;
        (*slot).owner_program_generation = owner.program_generation;
        ptr::copy_nonoverlapping(
            raw.as_ptr(),
            ptr::addr_of_mut!((*slot).raw_path).cast(),
            raw.len(),
        );
        ptr::copy_nonoverlapping(
            name.as_ptr(),
            ptr::addr_of_mut!((*slot).name).cast(),
            name.len(),
        );
    }
}
pub(super) unsafe fn name<'a>(slot: *const Slot) -> &'a [u8] {
    unsafe { core::slice::from_raw_parts(ptr::addr_of!((*slot).name).cast(), (*slot).name_len) }
}
pub(super) unsafe fn volume(slot: *const Slot) -> Option<fs::Volume> {
    unsafe {
        fs::volume(if (*slot).on_boot_volume {
            0
        } else {
            (*slot).drive_letter
        })
    }
}
pub(super) unsafe fn clear(slot: *mut Slot) {
    unsafe {
        if let Some(token) = (*slot).volume_use {
            end_use(token);
        }
        (*slot).volume_use = None;
        (*slot).active = false;
        (*slot).ready = false;
        (*slot).finished = false;
        (*slot).publish_started = false;
        (*slot).publish_target_name_len = 0;
        (*slot).publish_backup_name_len = 0;
        (*slot).publish_claim_generation = 0;
        (*slot).publish_declared = false;
        (*slot).publish_declared_parent_len = 0;
        (*slot).publish_protocol = 0;
        (*slot).raw_len = 0;
        (*slot).name_len = 0;
        (*slot).ancestor_count = 0;
        (*slot).ancestor_nodes.fill(0);
        (*slot).ancestor_generations.fill(0);
        (*slot).file_node = 0;
        (*slot).file_node_generation = 0;
        (*slot).has_file_node = false;
        (*slot).identity_refresh_pending = false;
        (*slot).file_attr = 0;
        (*slot).file_reparse = false;
        (*slot).file_created_time = 0;
        (*slot).file_created_date = 0;
        (*slot).file_access_date = 0;
        (*slot).file_modified_time = 0;
        (*slot).file_modified_date = 0;
        (*slot).size = 0;
        (*slot).flags = 0;
        (*slot).generation = 0;
        (*slot).owner_kind = 1;
        (*slot).owner_id = 0;
        (*slot).owner_generation = 0;
        (*slot).owner_program_id = 0;
        (*slot).owner_program_generation = 0;
        release(slot);
    }
}
pub(super) unsafe fn bind(slot: *mut Slot, entry: &fs::Entry) {
    unsafe {
        (*slot).file_node = entry.node;
        (*slot).file_node_generation = entry.generation;
        (*slot).has_file_node = true;
        (*slot).identity_refresh_pending = false;
        (*slot).file_attr = entry.attr;
        (*slot).file_reparse = entry.reparse;
        (*slot).file_created_time = entry.times[0];
        (*slot).file_created_date = entry.times[1];
        (*slot).file_access_date = entry.times[2];
        (*slot).file_modified_time = entry.times[3];
        (*slot).file_modified_date = entry.times[4];
        (*slot).name_len = entry.name_len.min(1024);
        ptr::copy_nonoverlapping(
            entry.name.as_ptr(),
            ptr::addr_of_mut!((*slot).name).cast(),
            (*slot).name_len,
        );
    }
}
pub(super) unsafe fn fingerprint(slot: *const Slot, entry: &fs::Entry) -> bool {
    unsafe {
        (*slot).has_file_node
            && (*slot).file_node == entry.node
            && (*slot).file_node_generation == entry.generation
            && (*slot).size == entry.size
            && (*slot).file_attr == entry.attr
            && (*slot).file_reparse == entry.reparse
            && (*slot).file_created_time == entry.times[0]
            && (*slot).file_created_date == entry.times[1]
            && (*slot).file_access_date == entry.times[2]
            && (*slot).file_modified_time == entry.times[3]
            && (*slot).file_modified_date == entry.times[4]
    }
}
pub(super) unsafe fn owns_entry(slot: *const Slot, entry: &fs::Entry) -> bool {
    unsafe {
        if !(*slot).has_file_node || entry.is_dir() || entry.size != (*slot).size {
            return false;
        }
        if (*slot).file_node != 0
            || (*slot).file_node_generation != 0
            || entry.node != 0
            || entry.generation != 0
        {
            return (*slot).file_node == entry.node
                && (*slot).file_node_generation == entry.generation;
        }
        fingerprint(slot, entry)
    }
}
pub(super) unsafe fn bind_ancestors(
    slot: *mut Slot,
    volume: fs::Volume,
    parent_path: &[u8],
    expected: u64,
) -> bool {
    unsafe {
        (*slot).ancestor_count = 0;
        let mut normalized = [0u8; 1024];
        normalized[0] = b'\\';
        let mut len = 1;
        let mut ends = [core::mem::MaybeUninit::<usize>::uninit(); 260];
        let mut scan = 0;
        while scan < parent_path.len() {
            while scan < parent_path.len() && path::separator(parent_path[scan]) {
                scan += 1;
            }
            if scan >= parent_path.len() {
                break;
            }
            let mut end = scan;
            while end < parent_path.len() && !path::separator(parent_path[end]) {
                end += 1;
            }
            let component = &parent_path[scan..end];
            scan = end;
            if component == b"." {
                continue;
            }
            if component == b".." {
                if (*slot).ancestor_count != 0 {
                    (*slot).ancestor_count -= 1;
                    let count = (*slot).ancestor_count;
                    (*slot).ancestor_nodes[count] = 0;
                    (*slot).ancestor_generations[count] = 0;
                    len = if count == 0 {
                        1
                    } else {
                        ends[count - 1].assume_init()
                    };
                }
                continue;
            }
            let count = (*slot).ancestor_count;
            if count >= 260 {
                return false;
            }
            if len > 1 {
                if len >= normalized.len() {
                    return false;
                }
                normalized[len] = b'\\';
                len += 1;
            }
            if component.len() > normalized.len() - len {
                return false;
            }
            normalized[len..len + component.len()].copy_from_slice(component);
            len += component.len();
            ends[count].write(len);
            (*slot).ancestor_count += 1;
        }
        let mut parent = fs::scalar(volume, 2);
        if (*slot).ancestor_count == 0 {
            return expected == parent;
        }
        for index in 0..(*slot).ancestor_count {
            let start = if index == 0 {
                1
            } else {
                ends[index - 1].assume_init() + 1
            };
            let end = ends[index].assume_init();
            if start >= end || end > len {
                return false;
            }
            let mut entry = fs::Entry::EMPTY;
            if fs::lookup(volume, parent, &normalized[start..end], false, &mut entry) != 0
                || !entry.is_dir()
                || entry.reparse
                || entry.node == 0
            {
                return false;
            }
            (*slot).ancestor_nodes[index] = entry.node;
            (*slot).ancestor_generations[index] = entry.generation;
            parent = entry.node;
        }
        parent == expected
    }
}
pub(super) unsafe fn projection_locked(index: usize) -> Projection {
    assert!(index < CAPACITY);
    unsafe { PROJECTIONS.0.get().cast::<Projection>().add(index).read() }
}
