//! Original exact create-only handover/retry and durable claim integration.
//! No stale path or old visibility acknowledgement authorizes deletion.
use super::super::registry_format_types::Bytes;
use super::{fs, path, stream_state as s};
use core::{cell::UnsafeCell, ptr};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ClaimInput {
    pub volume: fs::Volume,
    pub parent: Bytes,
    pub staged: Bytes,
    pub target: Bytes,
    pub backup: Bytes,
    pub node: u64,
    pub node_generation: u16,
    pub size: u64,
    pub protocol: u8,
    pub generation: u64,
}
unsafe extern "C" {
    fn r4os_zig_sys_publish_claim(
        refresh: bool,
        input: *const ClaimInput,
        out_generation: *mut u64,
    ) -> bool;
    fn r4os_zig_sys_publish_retire(generation: u64) -> bool;
}
struct ParentBuffers(UnsafeCell<[[u8; 1026]; 26]>);
// SAFETY: Each scratch buffer belongs to its matching admitted filesystem
// lane. Independent lanes cannot overwrite a borrowed claim parent over I/O.
unsafe impl Sync for ParentBuffers {}
static PARENTS: ParentBuffers = ParentBuffers(UnsafeCell::new([[0; 1026]; 26]));
pub(super) unsafe fn parent<'a>(letter: u8, raw: &[u8]) -> Option<&'a [u8]> {
    unsafe {
        let parent = path::parent(raw);
        if parent.len() + 2 > 1026 {
            return None;
        }
        let lane = letter.to_ascii_uppercase();
        if !lane.is_ascii_uppercase() {
            return None;
        }
        let output = PARENTS
            .0
            .get()
            .cast::<[u8; 1026]>()
            .add(usize::from(lane - b'A'))
            .cast::<u8>();
        output.write(letter);
        output.add(1).write(b':');
        let mut count = 2;
        for &ch in parent {
            output.add(count).write(if ch == b'/' { b'\\' } else { ch });
            count += 1;
        }
        if count < 3 {
            None
        } else {
            Some(core::slice::from_raw_parts(output, count))
        }
    }
}
// Keep the original claim operands explicit at the unchanged leaf boundary.
#[allow(clippy::too_many_arguments)]
pub(super) unsafe fn claim(
    refresh: bool,
    volume: fs::Volume,
    parent: &[u8],
    staged: &[u8],
    target: &[u8],
    backup: &[u8],
    node: u64,
    node_generation: u16,
    size: u64,
    protocol: u8,
    generation: u64,
) -> Option<u64> {
    unsafe {
        let input = ClaimInput {
            volume,
            parent: Bytes::from_slice(parent),
            staged: Bytes::from_slice(staged),
            target: Bytes::from_slice(target),
            backup: Bytes::from_slice(backup),
            node,
            node_generation,
            size,
            protocol,
            generation,
        };
        let mut output = 0;
        if r4os_zig_sys_publish_claim(refresh, &input, &mut output) {
            Some(output)
        } else {
            None
        }
    }
}
pub(super) unsafe fn retire(generation: u64) -> bool {
    unsafe { r4os_zig_sys_publish_retire(generation) }
}
pub(super) unsafe fn target<'a>(slot: *const s::Slot) -> &'a [u8] {
    unsafe {
        core::slice::from_raw_parts(
            ptr::addr_of!((*slot).publish_target_name).cast(),
            (*slot).publish_target_name_len,
        )
    }
}
pub(super) unsafe fn backup<'a>(slot: *const s::Slot) -> &'a [u8] {
    unsafe {
        core::slice::from_raw_parts(
            ptr::addr_of!((*slot).publish_backup_name).cast(),
            (*slot).publish_backup_name_len,
        )
    }
}
pub(super) unsafe fn remember(slot: *mut s::Slot, target: &[u8], backup: &[u8]) -> bool {
    unsafe {
        if target.is_empty() || target.len() > 768 || backup.is_empty() || backup.len() > 12 {
            return false;
        }
        (*slot).publish_target_name.fill(0);
        (*slot).publish_backup_name.fill(0);
        ptr::copy_nonoverlapping(
            target.as_ptr(),
            ptr::addr_of_mut!((*slot).publish_target_name).cast(),
            target.len(),
        );
        ptr::copy_nonoverlapping(
            backup.as_ptr(),
            ptr::addr_of_mut!((*slot).publish_backup_name).cast(),
            backup.len(),
        );
        (*slot).publish_target_name_len = target.len();
        (*slot).publish_backup_name_len = backup.len();
        (*slot).publish_started = true;
        true
    }
}
pub(super) unsafe fn names_match(slot: *const s::Slot, target: &[u8], backup: &[u8]) -> bool {
    unsafe {
        self::target(slot).eq_ignore_ascii_case(target)
            && self::backup(slot).eq_ignore_ascii_case(backup)
    }
}
pub(super) unsafe fn tuple_matches(slot: *const s::Slot, target: &[u8], backup: &[u8]) -> bool {
    unsafe { (*slot).publish_started && names_match(slot, target, backup) }
}
unsafe fn same_owned(
    volume: fs::Volume,
    slot: *const s::Slot,
    left: &fs::Entry,
    right: &fs::Entry,
) -> bool {
    unsafe {
        if !s::owns_entry(slot, left) || !s::owns_entry(slot, right) {
            return false;
        }
        fs::identity(volume, left, right, 1)
            || left.node == 0
                && right.node == 0
                && left.generation == 0
                && right.generation == 0
                && left.size == 0
                && right.size == 0
                && left.attr == right.attr
                && left.reparse == right.reparse
                && left.times == right.times
    }
}
#[derive(Clone, Copy)]
enum Observed {
    Stage,
    Aliased,
    Target,
    Missing,
    Conflict,
    Io,
}
unsafe fn inspect(
    volume: fs::Volume,
    parent: u64,
    target: &[u8],
    staged: &[u8],
    backup: &[u8],
    slot: *const s::Slot,
) -> Observed {
    unsafe {
        let mut target_entry = fs::Entry::EMPTY;
        let mut stage_entry = fs::Entry::EMPTY;
        let mut backup_entry = fs::Entry::EMPTY;
        let t = fs::lookup(volume, parent, target, true, &mut target_entry);
        let s = fs::lookup(volume, parent, staged, true, &mut stage_entry);
        let b = fs::lookup(volume, parent, backup, true, &mut backup_entry);
        if t == 2 || s == 2 || b == 2 {
            return Observed::Io;
        }
        if b == 0 {
            return Observed::Conflict;
        }
        if t == 0 && !s::owns_entry(slot, &target_entry)
            || s == 0 && !s::owns_entry(slot, &stage_entry)
        {
            return Observed::Conflict;
        }
        if t == 0 && s == 0 {
            return if same_owned(volume, slot, &target_entry, &stage_entry) {
                Observed::Aliased
            } else {
                Observed::Conflict
            };
        }
        if t == 0 {
            Observed::Target
        } else if s == 0 {
            Observed::Stage
        } else {
            Observed::Missing
        }
    }
}
pub(super) unsafe fn run(
    volume: fs::Volume,
    parent: u64,
    target: &[u8],
    staged: &[u8],
    backup: &[u8],
    slot: *const s::Slot,
) -> i32 {
    unsafe {
        let mut attempts = 0;
        while attempts < 2 {
            match inspect(volume, parent, target, staged, backup, slot) {
                Observed::Target => return if fs::flush(volume) { 0 } else { -6 },
                Observed::Stage | Observed::Aliased => {}
                Observed::Missing => return -2,
                Observed::Conflict => return -4,
                Observed::Io => return -6,
            }
            let result = fs::replace(volume, parent, target, staged, backup, true, false);
            if result != -6 {
                return result;
            }
            attempts += 1;
        }
        match inspect(volume, parent, target, staged, backup, slot) {
            Observed::Target => {
                if fs::flush(volume) {
                    0
                } else {
                    -6
                }
            }
            Observed::Missing => -2,
            Observed::Conflict => -4,
            _ => -6,
        }
    }
}
pub(super) unsafe fn settle(slot: *mut s::Slot) -> bool {
    unsafe {
        if !(*slot).active || !(*slot).publish_started {
            return true;
        }
        if (*slot).publish_target_name_len == 0 || (*slot).publish_backup_name_len == 0 {
            return false;
        }
        let Some(volume) = s::volume(slot) else {
            return false;
        };
        let result = run(
            volume,
            (*slot).parent_node,
            target(slot),
            s::name(slot),
            backup(slot),
            slot,
        );
        if matches!(result, 0 | -2) {
            if (*slot).publish_claim_generation != 0 && !retire((*slot).publish_claim_generation) {
                return false;
            }
            s::clear(slot);
            true
        } else {
            false
        }
    }
}
pub(super) unsafe fn refresh(slot: *mut s::Slot) {
    unsafe {
        if !(*slot).publish_declared
            || (*slot).publish_claim_generation == 0
            || (*slot).publish_declared_parent_len == 0
        {
            return;
        }
        let Some(volume) = s::volume(slot) else {
            return;
        };
        let parent = core::slice::from_raw_parts(
            ptr::addr_of!((*slot).publish_declared_parent).cast(),
            (*slot).publish_declared_parent_len,
        );
        claim(
            true,
            volume,
            parent,
            s::name(slot),
            target(slot),
            backup(slot),
            (*slot).file_node,
            (*slot).file_node_generation,
            (*slot).size,
            (*slot).publish_protocol,
            (*slot).publish_claim_generation,
        );
    }
}
