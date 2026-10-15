//! Original single-canonical-name proofs used by rename and atomic publish.
use super::{
    ntfs_format_names as text, ntfs_format_records as fmt, ntfs_format_types::*,
    ntfs_volume_attributes as a, ntfs_volume_build_names as names, ntfs_volume_edit as edit,
    ntfs_volume_records as r, ntfs_volume_types::*,
};
use core::ptr;

pub(super) unsafe fn equal(v: Volume, x: View, y: View) -> Option<bool> {
    unsafe {
        let mut left = [0u8; 510];
        let mut right = [0u8; 510];
        let n = text::to_utf16(x.get(), &mut left)?;
        let m = text::to_utf16(y.get(), &mut right)?;
        Some(text::compare(v.upcase.get(), &left[..n], &right[..m]) == 0)
    }
}

#[inline(never)]
pub(super) unsafe fn status(
    v: Volume,
    number: u64,
    sequence: u16,
    parent: u64,
    parent_sequence: u16,
    name: View,
) -> Lookup {
    unsafe {
        let wanted = ptr::addr_of_mut!((*v.scratch()).name_utf16).cast::<u8>();
        let Some(n) = text::to_utf16(name.get(), core::slice::from_raw_parts_mut(wanted, 510))
        else {
            return Lookup::Io;
        };
        let Some(h) = r::load(v, number, v.part_record()) else {
            return Lookup::Io;
        };
        if h.sequence != sequence || !a::stream_valid(r::view(v.part_record()), h) {
            return Lookup::Io;
        }
        if h.link_count != 1 {
            return Lookup::NotFound;
        }
        let mut offset = h.attrs_offset as usize;
        let mut filename_count = 0;
        let mut canonical_count = 0;
        let mut matches = false;
        while let Some(attr) = fmt::attribute(r::view(v.part_record()), &mut offset) {
            if attr.attr_type != 0x30 {
                continue;
            }
            filename_count += 1;
            let Some(value) = fmt::file_name(attr.value.get()) else {
                return Lookup::Io;
            };
            if value.namespace == 2 {
                continue;
            }
            canonical_count += 1;
            if value.parent.record == parent
                && value.parent.sequence == parent_sequence
                && text::compare(
                    v.upcase.get(),
                    value.name.get(),
                    core::slice::from_raw_parts(wanted, n),
                ) == 0
            {
                matches = true;
            }
        }
        if filename_count == 1 && canonical_count == 1 && matches {
            Lookup::Found
        } else {
            Lookup::NotFound
        }
    }
}

#[inline(never)]
pub(super) unsafe fn exact(v: Volume, number: u64, sequence: u16, expected: View) -> Lookup {
    unsafe {
        if fmt::file_name(expected.get()).is_none() {
            return Lookup::Io;
        }
        let Some(h) = r::load(v, number, v.part_record()) else {
            return Lookup::Io;
        };
        if h.sequence != sequence
            || h.link_count != 1
            || !a::stream_valid(r::view(v.part_record()), h)
        {
            return Lookup::Io;
        }
        let mut offset = h.attrs_offset as usize;
        let mut count = 0;
        let mut exact = 0;
        while let Some(attr) = fmt::attribute(r::view(v.part_record()), &mut offset) {
            if attr.attr_type != 0x30 {
                continue;
            }
            count += 1;
            if attr.value.get() == expected.get() {
                exact += 1;
            }
        }
        if count == 1 && exact == 1 {
            Lookup::Found
        } else {
            Lookup::NotFound
        }
    }
}

#[inline(never)]
pub(super) unsafe fn rewrite(
    v: Volume,
    entry: LookupResult,
    parent: u64,
    parent_sequence: u16,
    target: View,
) -> WriteStatus {
    unsafe {
        let data = ptr::addr_of_mut!((*v.scratch()).attr_op);
        if r::collect(v, entry.record, 0x80, View::default(), data) != Lookup::Found {
            return WriteStatus::Io;
        }
        let mut filename = [0u8; 0x42 + 510];
        let Some(n) = names::value(
            v,
            &mut filename,
            parent,
            parent_sequence,
            target.get(),
            false,
            (*data).alloc_size,
            (*data).data_size,
            entry.entry.created_time_nt,
        ) else {
            return WriteStatus::Invalid;
        };
        let Some(mut h) = r::load(v, entry.record, v.write_record()) else {
            return WriteStatus::Io;
        };
        if h.sequence != entry.sequence
            || h.link_count != 1
            || !a::stream_valid(r::view(v.write_record()), h)
        {
            return WriteStatus::Io;
        }
        if !edit::resident(
            r::mutable(v.write_record()),
            &mut h,
            0x30,
            &[],
            &filename[..n],
        ) {
            return WriteStatus::RecordFull;
        }
        if r::store(v, entry.record, v.write_record()) {
            WriteStatus::Ok
        } else {
            WriteStatus::Io
        }
    }
}
