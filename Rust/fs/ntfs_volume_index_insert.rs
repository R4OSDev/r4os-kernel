//! Original bounded B+ tree insertion, root pushdown and median splitting.
use super::{
    ntfs_format_names as names, ntfs_format_types::*, ntfs_volume_edit as edit,
    ntfs_volume_index_alloc as allocation, ntfs_volume_index_io as idx, ntfs_volume_io as io,
    ntfs_volume_records as r, ntfs_volume_types::*,
};
use core::ptr;
pub(super) unsafe fn entry_a(v: Volume, n: usize) -> Buffer {
    assert!(n <= ENTRY_MAX);
    unsafe {
        Buffer {
            data: ptr::addr_of_mut!((*v.scratch()).entry_a).cast(),
            len: n,
        }
    }
}
unsafe fn full(v: Volume) -> Buffer {
    unsafe {
        Buffer {
            data: ptr::addr_of_mut!((*v.scratch()).block).cast(),
            len: 4096 + ENTRY_MAX,
        }
    }
}
unsafe fn second(v: Volume) -> Buffer {
    assert!(v.index_block_bytes <= 4096);
    unsafe {
        Buffer {
            data: ptr::addr_of_mut!((*v.scratch()).block2).cast(),
            len: v.index_block_bytes as usize,
        }
    }
}
#[inline(never)]
pub(super) unsafe fn root_insert(v: Volume, dir: u64, entry: View) -> WriteStatus {
    unsafe {
        let Some(h0) = r::load(v, dir, v.record()) else {
            return WriteStatus::Io;
        };
        let record = v.record();
        let Some(root) = idx::Root::locate(r::view(record), h0) else {
            return WriteStatus::Invalid;
        };
        let bytes = r::view(record);
        let at = root.header_at + le32(bytes, root.header_at) as usize;
        let length = le32(bytes, root.header_at + 4);
        let end = root.header_at + length as usize;
        if le32(bytes, root.span.offset + 16) as usize + entry.len > 0x180 {
            return WriteStatus::DirFull;
        }
        let key = idx::entry_name(entry.get(), 0);
        let mut pos = at;
        while pos < end {
            if idx::entry_end(r::view(record), pos)
                || names::compare(v.upcase.get(), key, idx::entry_name(r::view(record), pos)) < 0
            {
                break;
            }
            pos += idx::entry_len(r::view(record), pos)
        }
        let mut h = h0;
        let mut span = root.span;
        let needed = span.length + entry.len;
        if !edit::resize(r::mutable(record), &mut h, &mut span, needed) {
            return WriteStatus::DirFull;
        }
        let tail = root.header_at + le32(r::view(record), root.header_at + 4) as usize;
        r::mutable(record).copy_within(pos..tail, pos + entry.len);
        ptr::copy_nonoverlapping(entry.data, record.data.add(pos), entry.len);
        r::put32(
            r::mutable(record),
            root.header_at + 4,
            length + entry.len as u32,
        );
        let old = le32(r::view(record), root.header_at + 8);
        r::put32(
            r::mutable(record),
            root.header_at + 8,
            old + entry.len as u32,
        );
        let old = le32(r::view(record), root.span.offset + 16);
        r::put32(
            r::mutable(record),
            root.span.offset + 16,
            old + entry.len as u32,
        );
        if !r::store(v, dir, record) {
            WriteStatus::Io
        } else {
            WriteStatus::Ok
        }
    }
}
#[inline(never)]
pub(super) unsafe fn push_down(v: Volume, dir: u64) -> WriteStatus {
    unsafe {
        let Some(vcn) = allocation::allocate(v, dir) else {
            return WriteStatus::NoSpace;
        };
        let alloc = ptr::addr_of_mut!((*v.scratch()).attr_index);
        if r::collect(v, dir, 0xa0, View::of(&idx::I30), alloc) != Lookup::Found {
            return WriteStatus::Io;
        }
        let Some(h0) = r::load(v, dir, v.record()) else {
            return WriteStatus::Io;
        };
        let record = v.record();
        let Some(root) = idx::Root::locate(r::view(record), h0) else {
            return WriteStatus::Invalid;
        };
        let at = root.header_at + le32(r::view(record), root.header_at) as usize;
        let end = root.header_at + le32(r::view(record), root.header_at + 4) as usize;
        let b = second(v);
        ptr::write_bytes(b.data, 0, b.len);
        let n = end - at;
        if 0x40 + n > b.len {
            return WriteStatus::Invalid;
        }
        ptr::copy_nonoverlapping(record.data.add(at), b.data.add(0x40), n);
        let mut children = false;
        let mut pos = 0x40;
        while pos < 0x40 + n {
            if idx::entry_child(r::view(b), pos).is_some() {
                children = true
            }
            if idx::entry_end(r::view(b), pos) {
                break;
            }
            pos += idx::entry_len(r::view(b), pos)
        }
        idx::header(v, r::mutable(b), vcn, 0x40 + n, children);
        if !idx::store_from(v, dir, r::attr_runs(alloc), vcn, b) {
            return WriteStatus::Io;
        }
        let mut value = [0u8; 0x38];
        ptr::copy_nonoverlapping(
            record.data.add(root.span.offset + root.value_offset),
            value.as_mut_ptr(),
            16,
        );
        r::put32(&mut value, 16, 16);
        r::put32(&mut value, 20, 0x28);
        r::put32(&mut value, 24, 0x28);
        value[28] = 1;
        r::put16(&mut value, 0x28, 0x18);
        r::put16(&mut value, 0x2c, 3);
        r::put64(&mut value, 0x30, vcn);
        let mut h = h0;
        if !edit::resident(r::mutable(record), &mut h, 0x90, &idx::I30, &value)
            || !r::store(v, dir, record)
        {
            WriteStatus::Io
        } else {
            WriteStatus::Ok
        }
    }
}
pub(super) unsafe fn root_child(v: Volume, dir: u64) -> Option<u64> {
    unsafe {
        let h = r::load(v, dir, v.record())?;
        let b = r::view(v.record());
        let root = idx::Root::locate(b, h)?;
        let mut pos = root.header_at + le32(b, root.header_at) as usize;
        let end = root.header_at + le32(b, root.header_at + 4) as usize;
        while pos < end {
            if idx::entry_end(b, pos) {
                return idx::entry_child(b, pos);
            }
            pos += idx::entry_len(b, pos)
        }
        None
    }
}
pub(super) unsafe fn insert_slack(v: Volume, entry: View) {
    unsafe {
        let b = full(v);
        let at = 24 + le32(r::view(b), 24) as usize;
        let length = le32(r::view(b), 28);
        let end = 24 + length as usize;
        let key = idx::entry_name(entry.get(), 0);
        let mut pos = at;
        while pos < end {
            if idx::entry_end(r::view(b), pos)
                || names::compare(v.upcase.get(), key, idx::entry_name(r::view(b), pos)) < 0
            {
                break;
            }
            pos += idx::entry_len(r::view(b), pos)
        }
        r::mutable(b).copy_within(pos..end, pos + entry.len);
        ptr::copy_nonoverlapping(entry.data, b.data.add(pos), entry.len);
        r::put32(r::mutable(b), 28, length + entry.len as u32)
    }
}
pub(super) unsafe fn fits(v: Volume) -> bool {
    unsafe {
        let b = r::view(full(v));
        le32(b, 28) <= le32(b, 32)
    }
}
#[inline(never)]
pub(super) unsafe fn split(v: Volume, dir: u64, runs: io::Runs, vcn: u64, new_vcn: u64) -> bool {
    unsafe {
        let b = full(v);
        let at = 24 + le32(r::view(b), 24) as usize;
        let end = 24 + le32(r::view(b), 28) as usize;
        let mut end_pos = at;
        let mut count = 0;
        while end_pos < end && !idx::entry_end(r::view(b), end_pos) {
            count += 1;
            end_pos += idx::entry_len(r::view(b), end_pos)
        }
        if count < 3 {
            return false;
        }
        let payload = end_pos - at;
        let mut median = at;
        let mut i = 0;
        let mut sum = 0;
        while i + 1 < count {
            let n = idx::entry_len(r::view(b), median);
            if sum + n >= payload / 2 && i > 0 {
                break;
            }
            sum += n;
            median += n;
            i += 1
        }
        let median_len = idx::entry_len(r::view(b), median);
        let child = idx::entry_child(r::view(b), median);
        let other = second(v);
        ptr::write_bytes(other.data, 0, other.len);
        let left = median - at;
        ptr::copy_nonoverlapping(b.data.add(at), other.data.add(0x40), left);
        let mut children = child.is_some();
        let mut scan = 0x40;
        while scan < 0x40 + left {
            if idx::entry_child(r::view(other), scan).is_some() {
                children = true
            }
            scan += idx::entry_len(r::view(other), scan)
        }
        let mut offset = 0x40 + left;
        let end_len = if child.is_some() { 24 } else { 16 };
        r::put16(r::mutable(other), offset + 8, end_len as u16);
        r::put16(
            r::mutable(other),
            offset + 12,
            if child.is_some() { 3 } else { 2 },
        );
        if let Some(child) = child {
            r::put64(r::mutable(other), offset + end_len - 8, child)
        }
        offset += end_len;
        idx::header(v, r::mutable(other), new_vcn, offset, children);
        if !idx::store_from(v, dir, runs, new_vcn, other) {
            return false;
        }
        let filename = le16(r::view(b), median + 10) as usize;
        let plain = (16 + filename + 7) & !7usize;
        let n = plain + 8;
        if n > ENTRY_MAX {
            return false;
        }
        let promoted = entry_a(v, n);
        ptr::write_bytes(promoted.data, 0, n);
        ptr::copy_nonoverlapping(b.data.add(median), promoted.data, 16);
        ptr::copy_nonoverlapping(b.data.add(median + 16), promoted.data.add(16), filename);
        r::put16(r::mutable(promoted), 8, n as u16);
        r::put16(r::mutable(promoted), 12, 1);
        r::put64(r::mutable(promoted), n - 8, new_vcn);
        let rest = median + median_len;
        let rest_len = end - rest;
        r::mutable(b).copy_within(rest..end, at);
        r::put32(r::mutable(b), 28, (at - 24 + rest_len) as u32);
        idx::store(v, dir, runs, vcn)
    }
}
#[inline(never)]
pub(super) unsafe fn descent(
    v: Volume,
    dir: u64,
    key: View,
    runs: io::Runs,
    path: &mut [u64; 8],
) -> Option<usize> {
    unsafe {
        let h = r::load(v, dir, v.record())?;
        let record = v.record();
        let root = idx::Root::locate(r::view(record), h)?;
        let mut pos = root.header_at + le32(r::view(record), root.header_at) as usize;
        let end = root.header_at + le32(r::view(record), root.header_at + 4) as usize;
        let mut next = None;
        while pos < end {
            if idx::entry_end(r::view(record), pos)
                || names::compare(
                    v.upcase.get(),
                    key.get(),
                    idx::entry_name(r::view(record), pos),
                ) < 0
            {
                next = idx::entry_child(r::view(record), pos);
                break;
            }
            pos += idx::entry_len(r::view(record), pos)
        }
        let mut depth = 0;
        while depth < 8 {
            let Some(vcn) = next else { return Some(depth) };
            path[depth] = vcn;
            depth += 1;
            if !idx::load(v, runs, vcn) {
                return None;
            }
            let b = idx::block(v);
            if !idx::has_children(r::view(b)) {
                return Some(depth);
            }
            let mut pos = 24 + le32(r::view(b), 24) as usize;
            let end = 24 + le32(r::view(b), 28) as usize;
            next = None;
            while pos < end {
                if idx::entry_end(r::view(b), pos)
                    || names::compare(v.upcase.get(), key.get(), idx::entry_name(r::view(b), pos))
                        < 0
                {
                    next = idx::entry_child(r::view(b), pos);
                    break;
                }
                pos += idx::entry_len(r::view(b), pos)
            }
            next?;
        }
        None
    }
}
#[inline(never)]
pub(super) unsafe fn insert(v: Volume, dir: u64, reference: u64, key: View) -> WriteStatus {
    unsafe {
        if key.len > ENTRY_MAX {
            return WriteStatus::Invalid;
        }
        let out = entry_a(v, key.len);
        ptr::copy_nonoverlapping(key.data, out.data, key.len);
        r::put64(r::mutable(out), 0, reference);
        prepared(v, dir, key.len)
    }
}
#[inline(never)]
pub(super) unsafe fn prepared(v: Volume, dir: u64, len: usize) -> WriteStatus {
    unsafe {
        {
            let Some(h) = r::load(v, dir, v.record()) else {
                return WriteStatus::Io;
            };
            let Some(root) = idx::Root::locate(r::view(v.record()), h) else {
                return WriteStatus::Invalid;
            };
            if r::view(v.record())[root.header_at + 12] & 1 == 0 {
                let status = root_insert(
                    v,
                    dir,
                    View {
                        data: entry_a(v, len).data,
                        len,
                    },
                );
                if status != WriteStatus::DirFull {
                    return status;
                }
                let status = push_down(v, dir);
                if status != WriteStatus::Ok {
                    return status;
                }
            }
        }
        let alloc = ptr::addr_of_mut!((*v.scratch()).attr_index);
        if r::collect(v, dir, 0xa0, View::of(&idx::I30), alloc) != Lookup::Found {
            return WriteStatus::Io;
        }
        let mut path = [0u64; 8];
        let name = View::of(idx::entry_name(r::view(entry_a(v, len)), 0));
        let Some(depth) = descent(v, dir, name, r::attr_runs(alloc), &mut path) else {
            return WriteStatus::Io;
        };
        if depth == 0 {
            return WriteStatus::Io;
        }
        let mut level = depth - 1;
        let mut current = len;
        for _ in 0..10 {
            if !idx::load(v, r::attr_runs(alloc), path[level]) {
                return WriteStatus::Io;
            }
            insert_slack(
                v,
                View {
                    data: entry_a(v, current).data,
                    len: current,
                },
            );
            if fits(v) {
                return if idx::store(v, dir, r::attr_runs(alloc), path[level]) {
                    WriteStatus::Ok
                } else {
                    WriteStatus::Io
                };
            }
            let Some(new_vcn) = allocation::allocate(v, dir) else {
                return WriteStatus::NoSpace;
            };
            if r::collect(v, dir, 0xa0, View::of(&idx::I30), alloc) != Lookup::Found
                || !split(v, dir, r::attr_runs(alloc), path[level], new_vcn)
            {
                return WriteStatus::Io;
            }
            current = idx::entry_len(r::view(entry_a(v, ENTRY_MAX)), 0);
            if level == 0 {
                let status = root_insert(
                    v,
                    dir,
                    View {
                        data: entry_a(v, current).data,
                        len: current,
                    },
                );
                if status != WriteStatus::DirFull {
                    return status;
                }
                let status = push_down(v, dir);
                if status != WriteStatus::Ok {
                    return status;
                }
                if r::collect(v, dir, 0xa0, View::of(&idx::I30), alloc) != Lookup::Found {
                    return WriteStatus::Io;
                }
                let Some(child) = root_child(v, dir) else {
                    return WriteStatus::Io;
                };
                if !idx::load(v, r::attr_runs(alloc), child) {
                    return WriteStatus::Io;
                }
                insert_slack(
                    v,
                    View {
                        data: entry_a(v, current).data,
                        len: current,
                    },
                );
                if !fits(v) || !idx::store(v, dir, r::attr_runs(alloc), child) {
                    return WriteStatus::Io;
                }
                return WriteStatus::Ok;
            }
            level -= 1;
        }
        WriteStatus::Io
    }
}
