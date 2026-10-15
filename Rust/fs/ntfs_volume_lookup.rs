//! Original statusful namespace traversal, exact record/link identities.
use super::{
    ntfs_format_names as text, ntfs_format_records as fmt, ntfs_format_types::*,
    ntfs_volume_attributes as a, ntfs_volume_index_io as idx, ntfs_volume_names as names,
    ntfs_volume_records as r, ntfs_volume_types::*,
};
#[inline(never)]
pub(super) unsafe fn record_link(v: Volume, number: u64) -> Option<(u16, u16)> {
    unsafe {
        let h = r::load(v, number, v.part_record())?;
        if number > u64::from(u32::MAX)
            || h.flags & 1 == 0
            || u64::from(h.record_number) != number
            || !a::stream_valid(r::view(v.part_record()), h)
        {
            return None;
        }
        Some((h.sequence, h.link_count))
    }
}
pub(super) unsafe fn single_link(v: Volume, number: u64, sequence: u16) -> WriteStatus {
    unsafe {
        match record_link(v, number) {
            Some((s, n)) if s == sequence => {
                if n == 1 {
                    WriteStatus::Ok
                } else {
                    WriteStatus::Unsupported
                }
            }
            _ => WriteStatus::Io,
        }
    }
}
pub(super) unsafe fn sequence(v: Volume, number: u64) -> Option<u16> {
    unsafe { record_link(v, number).map(|x| x.0) }
}
pub(super) unsafe fn live(v: Volume, found: LookupResult) -> bool {
    unsafe {
        let Some(h) = r::load(v, found.record, v.part_record()) else {
            return false;
        };
        found.record <= u64::from(u32::MAX)
            && h.flags & 1 != 0
            && u64::from(h.record_number) == found.record
            && h.sequence == found.sequence
            && (h.flags & 2 != 0) == found.entry.is_dir()
            && a::stream_valid(r::view(v.part_record()), h)
    }
}
pub(super) unsafe fn identity(v: Volume, number: u64, sequence: u16, directory: bool) -> Lookup {
    unsafe {
        let Some(h) = r::load(v, number, v.part_record()) else {
            return Lookup::Io;
        };
        if number > u64::from(u32::MAX)
            || h.flags & 1 == 0
            || u64::from(h.record_number) != number
            || h.sequence != sequence
            || (h.flags & 2 != 0) != directory
            || !a::stream_valid(r::view(v.part_record()), h)
        {
            Lookup::Io
        } else {
            Lookup::Found
        }
    }
}
pub(super) unsafe fn has_link(
    v: Volume,
    record: Buffer,
    h: FileRecordHeader,
    parent: u64,
    parent_sequence: u16,
    wanted: View,
) -> bool {
    unsafe {
        let mut offset = usize::from(h.attrs_offset);
        while let Some(attr) = fmt::attribute(r::view(record), &mut offset) {
            if attr.attr_type != 0x30 {
                continue;
            }
            let Some(name) = fmt::file_name(attr.value.get()) else {
                return false;
            };
            if name.parent.record == parent
                && name.parent.sequence == parent_sequence
                && text::compare(v.upcase.get(), name.name.get(), wanted.get()) == 0
            {
                return true;
            }
        }
        false
    }
}
#[inline(never)]
pub(super) unsafe fn lookup(
    v: Volume,
    dir: u64,
    name: View,
    out: *mut LookupResult,
    transient: bool,
    depth: u32,
) -> Lookup {
    unsafe {
        if name.len == 0 || name.len > 768 {
            return Lookup::Io;
        }
        let target_buf = core::ptr::addr_of_mut!((*v.scratch()).name_utf16).cast::<u8>();
        let Some(n) = text::to_utf16(name.get(), core::slice::from_raw_parts_mut(target_buf, 510))
        else {
            return Lookup::Io;
        };
        let target = View {
            data: target_buf,
            len: n,
        };
        if !v.cache().is_null() {
            if transient {
                (*v.cache()).note_recovery_bypass()
            } else {
                match (*v.cache()).lookup_path(
                    v.upcase.get(),
                    dir,
                    target.get(),
                    v.cache_now_ticks,
                    &mut *out,
                ) {
                    PathLookup::Positive => return Lookup::Found,
                    PathLookup::Negative => return names::not_found(depth, 10),
                    PathLookup::Miss => {}
                }
                (*v.cache()).note_tree_walk()
            }
        }
        let result = uncached(v, dir, target, out, transient, depth);
        if !transient && !v.cache().is_null() {
            (*v.cache()).store_path(
                dir,
                target.get(),
                result,
                if result == Lookup::Found {
                    &*out
                } else {
                    &LookupResult::EMPTY
                },
                v.cache_now_ticks,
            )
        }
        result
    }
}
#[inline(never)]
unsafe fn uncached(
    v: Volume,
    dir: u64,
    target: View,
    out: *mut LookupResult,
    transient: bool,
    depth: u32,
) -> Lookup {
    unsafe {
        let Some(h) = r::load(v, dir, v.record()) else {
            return Lookup::Io;
        };
        if dir > u64::from(u32::MAX)
            || h.flags & 1 == 0
            || u64::from(h.record_number) != dir
            || h.flags & 2 == 0
        {
            return Lookup::Io;
        }
        let record = v.record();
        if !a::stream_valid(r::view(record), h) {
            return Lookup::Io;
        }
        let Some(root_attr) = r::find(v, record, h, 0x90, View::of(&idx::I30)) else {
            return Lookup::Io;
        };
        let Some(root) = fmt::index_root(root_attr.value.get()) else {
            return Lookup::Io;
        };
        if root.indexed_attr_type != 0x30 || root.collation_rule != 1 {
            return Lookup::Io;
        }
        let mut alloc = AttrScratch::EMPTY;
        let mut have = false;
        if root.header.flags & 1 != 0 {
            if r::collect(v, dir, 0xa0, View::of(&idx::I30), &mut alloc) != Lookup::Found
                || alloc.resident
                || alloc.count == 0
            {
                return Lookup::Io;
            }
            have = true;
            let Some(h2) = r::load(v, dir, v.record()) else {
                return Lookup::Io;
            };
            if !a::stream_valid(r::view(record), h2) {
                return Lookup::Io;
            }
        }
        let Some(h2) = fmt::file_header(r::view(record)) else {
            return Lookup::Io;
        };
        let Some(root_attr) = r::find(v, record, h2, 0x90, View::of(&idx::I30)) else {
            return Lookup::Io;
        };
        let Some(root) = fmt::index_root(root_attr.value.get()) else {
            return Lookup::Io;
        };
        let mut node = root.entries;
        let mut in_root = true;
        let mut current = 0;
        for _ in 0..8 {
            let mut offset = 0;
            let mut next = None;
            let mut routed = false;
            let mut it = IteratorState::default();
            while let Some(entry) = fmt::index_entry(node.get(), &mut it) {
                if entry.flags & 2 != 0 {
                    if entry.key_length != 0 {
                        return Lookup::Io;
                    }
                    routed = true;
                    if entry.flags & 1 != 0 && entry.has_sub_node_vcn {
                        next = Some(entry.sub_node_vcn)
                    }
                    if next.is_none() {
                        return names::not_found(depth, 20);
                    }
                    break;
                }
                let Some(name) = fmt::file_name(entry.key.get()) else {
                    return Lookup::Io;
                };
                if name.parent.record != dir || name.parent.sequence != h.sequence {
                    return Lookup::Io;
                }
                let order = text::compare(v.upcase.get(), target.get(), name.name.get());
                if order == 0 {
                    let reference = FileReference::parse(entry.file_reference);
                    if names::hidden_system(dir, reference.record) {
                        return names::not_found(depth, 22);
                    }
                    let Some(child) = r::load(v, reference.record, v.part_record()) else {
                        return Lookup::Io;
                    };
                    if reference.record > u64::from(u32::MAX)
                        || child.flags & 1 == 0
                        || u64::from(child.record_number) != reference.record
                        || child.sequence != reference.sequence
                        || (child.flags & 2 != 0) != (name.flags & 0x1000_0000 != 0)
                        || !a::stream_valid(r::view(v.part_record()), child)
                    {
                        return Lookup::Io;
                    }
                    if !transient && !has_link(v, v.part_record(), child, dir, h.sequence, target) {
                        return Lookup::Io;
                    }
                    let Some(built) = names::entry_from_name(reference, name) else {
                        return Lookup::Io;
                    };
                    out.write(LookupResult {
                        record: reference.record,
                        sequence: reference.sequence,
                        entry: built,
                        in_root,
                        block_vcn: current,
                        entry_offset: offset,
                    });
                    return Lookup::Found;
                }
                if order < 0 {
                    routed = true;
                    if entry.flags & 1 != 0 && entry.has_sub_node_vcn {
                        next = Some(entry.sub_node_vcn)
                    }
                    if next.is_none() {
                        return names::not_found(depth, 21);
                    }
                    break;
                }
                offset += usize::from(entry.entry_length)
            }
            if !routed {
                return Lookup::Io;
            }
            let Some(vcn) = next else { return Lookup::Io };
            if !have || !idx::cached(v, dir, r::attr_runs(&alloc), vcn) {
                return Lookup::Io;
            }
            let Some(block) = fmt::index_block(r::view(idx::block(v))) else {
                return Lookup::Io;
            };
            if block.vcn != vcn {
                return Lookup::Io;
            }
            node = block.entries;
            in_root = false;
            current = vcn;
        }
        Lookup::Io
    }
}
#[inline(never)]
pub(super) unsafe fn resolve_path(v: Volume, path: View, out: *mut u64) -> Lookup {
    unsafe {
        let mut record = 5;
        let mut start = 0;
        let mut depth = 0;
        while start < path.len {
            while start < path.len && matches!(*path.data.add(start), b'\\' | b'/') {
                start += 1
            }
            if start >= path.len {
                break;
            }
            let mut end = start;
            while end < path.len && !matches!(*path.data.add(end), b'\\' | b'/') {
                end += 1
            }
            let segment = View {
                data: path.data.add(start),
                len: end - start,
            };
            if segment.len != 0 {
                depth += 1;
                if depth > 24 {
                    return Lookup::Io;
                }
                let mut found = LookupResult::EMPTY;
                let status = lookup(v, record, segment, &mut found, false, depth);
                if status != Lookup::Found {
                    return status;
                }
                if !found.entry.is_dir() {
                    return names::not_found(depth, 90);
                }
                if found.entry.reparse {
                    return Lookup::Io;
                }
                record = found.record
            }
            start = end
        }
        out.write(record);
        Lookup::Found
    }
}
#[inline(never)]
pub(super) unsafe fn resolve_entry(v: Volume, path: View, out: *mut ResolvedEntry) -> Lookup {
    unsafe {
        let mut dir = 5;
        let mut result: Option<Entry> = None;
        let mut parent = 5;
        let mut start = 0;
        let mut depth = 0;
        while start < path.len {
            while start < path.len && matches!(*path.data.add(start), b'\\' | b'/') {
                start += 1
            }
            if start >= path.len {
                break;
            }
            let mut end = start;
            while end < path.len && !matches!(*path.data.add(end), b'\\' | b'/') {
                end += 1
            }
            let segment = View {
                data: path.data.add(start),
                len: end - start,
            };
            if segment.len != 0 {
                depth += 1;
                if depth > 24 {
                    return Lookup::Io;
                }
                if let Some(previous) = result {
                    if !previous.is_dir() {
                        return names::not_found(depth, 90);
                    }
                    if previous.reparse {
                        return Lookup::Io;
                    }
                    dir = previous.record
                }
                let mut found = LookupResult::EMPTY;
                let status = lookup(v, dir, segment, &mut found, false, depth);
                if status != Lookup::Found {
                    return status;
                }
                parent = dir;
                result = Some(found.entry)
            }
            start = end
        }
        if let Some(mut entry) = result {
            if !entry.is_dir() && !entry.reparse {
                let mut attr = AttrScratch::EMPTY;
                if r::collect(v, entry.record, 0x80, View::default(), &mut attr) != Lookup::Found {
                    return Lookup::Io;
                }
                entry.size = attr.data_size
            }
            out.write(ResolvedEntry {
                entry,
                parent_record: parent,
            });
            Lookup::Found
        } else {
            names::not_found(0, 91)
        }
    }
}
#[inline(never)]
pub(super) unsafe fn parent_name(v: Volume, number: u64) -> Option<RecordName> {
    unsafe {
        let (info, links) = record_link(v, number)?;
        if links != 1 {
            return None;
        }
        let h = r::load(v, number, v.record())?;
        if h.sequence != info {
            return None;
        }
        let mut offset = usize::from(h.attrs_offset);
        while let Some(attr) = fmt::attribute(r::view(v.record()), &mut offset) {
            if attr.attr_type != 0x30 {
                continue;
            }
            let Some(name) = fmt::file_name(attr.value.get()) else {
                continue;
            };
            if name.namespace == 2 {
                continue;
            }
            let mut out = RecordName {
                parent: name.parent.record,
                parent_sequence: name.parent.sequence,
                name: [0; 768],
                name_len: 0,
            };
            out.name_len = text::to_utf8(name.name.get(), &mut out.name)?;
            if identity(v, out.parent, out.parent_sequence, true) != Lookup::Found {
                return None;
            }
            return Some(out);
        }
        None
    }
}
