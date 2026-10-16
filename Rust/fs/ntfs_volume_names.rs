//! Original NTFS entry projection and bounded lookup diagnosis.
use super::{
    ntfs_format_names as names,
    ntfs_format_types::{FileName, FileReference},
    ntfs_volume_types::*,
};
use core::sync::atomic::{AtomicU32, Ordering::Relaxed};
static LOOKUP_STAGE: AtomicU32 = AtomicU32::new(0);
static LOOKUP_SEQUENCE: AtomicU32 = AtomicU32::new(0);
pub fn lookup_stage() -> u32 {
    LOOKUP_STAGE.load(Relaxed)
}
pub fn not_found(depth: u32, phase: u32) -> Lookup {
    let code = depth.min(24) * 100 + phase;
    let old = LOOKUP_SEQUENCE
        .try_update(Relaxed, Relaxed, |old| {
            let next = old.wrapping_add(1) & 0x000f_ffff;
            Some(if next == 0 { 1 } else { next })
        })
        .expect("NTFS lookup sequence");
    let next = old.wrapping_add(1) & 0x000f_ffff;
    let next = if next == 0 { 1 } else { next };
    LOOKUP_STAGE.store((next << 12) | (code & 0x0fff), Relaxed);
    Lookup::NotFound
}
/// Error-only extension of the existing opaque diagnostic stage. Phases
///50..99 identify I/O rejection, retaining depth/sequence packing and status.
pub(super) fn io_failure(depth: u32, phase: u32) -> Lookup {
    assert!(phase < 50);
    let _ = not_found(depth, 50 + phase);
    Lookup::Io
}
pub(super) unsafe fn entry_from_name(reference: FileReference, name: FileName) -> Option<Entry> {
    unsafe {
        let mut entry = Entry {
            record: reference.record,
            sequence: reference.sequence,
            size: name.data_size,
            ..Entry::EMPTY
        };
        entry.name_len = names::to_utf8(name.name.get(), &mut entry.name)?;
        let mut attr = 0;
        if name.flags & 1 != 0 {
            attr |= 1
        }
        if name.flags & 2 != 0 {
            attr |= 2
        }
        if name.flags & 4 != 0 {
            attr |= 4
        }
        if name.flags & 0x20 != 0 {
            attr |= 0x20
        }
        if name.flags & 0x1000_0000 != 0 {
            attr |= 0x10;
            entry.size = 0
        }
        entry.reparse = name.flags & 0x400 != 0;
        entry.attr = attr;
        entry.created_time_nt = name.creation_time;
        entry.modified_time_nt = name.data_change_time;
        entry.access_time_nt = name.access_time;
        Some(entry)
    }
}
pub fn hidden_system(directory: u64, child: u64) -> bool {
    directory == 5 && child < 16
}
pub static APPEND_STAGE: AtomicU32 = AtomicU32::new(0);
pub fn append_stage() -> u32 {
    APPEND_STAGE.load(Relaxed)
}
pub fn append_at(stage: u32) {
    APPEND_STAGE.store(stage, Relaxed)
}
