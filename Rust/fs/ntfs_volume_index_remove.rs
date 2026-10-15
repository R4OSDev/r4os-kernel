//! Original exact-reference I30 detach and bounded empty-node retirement.
use super::{
    ntfs_format_names as names, ntfs_format_types::*, ntfs_volume_clusters as clusters,
    ntfs_volume_edit as edit, ntfs_volume_index_alloc as allocation,
    ntfs_volume_index_insert as insert, ntfs_volume_index_io as idx, ntfs_volume_io as io,
    ntfs_volume_records as r, ntfs_volume_types::*,
};
use core::ptr;
fn result(ok: bool) -> WriteStatus {
    if ok { WriteStatus::Ok } else { WriteStatus::Io }
}
unsafe fn full(v: Volume) -> Buffer {
    unsafe {
        Buffer {
            data: ptr::addr_of_mut!((*v.scratch()).block).cast(),
            len: 4096 + ENTRY_MAX,
        }
    }
}
#[inline(never)]
pub(super) unsafe fn remove(v: Volume, dir: u64, name: View, expected: Option<u64>) -> WriteStatus {
    unsafe {
        let out = ptr::addr_of_mut!((*v.scratch()).name_utf16).cast::<u8>();
        let Some(n) = names::to_utf16(name.get(), core::slice::from_raw_parts_mut(out, 510)) else {
            return WriteStatus::Invalid;
        };
        remove_utf16(v, dir, View { data: out, len: n }, expected)
    }
}
#[inline(never)]
pub(super) unsafe fn remove_utf16(
    v: Volume,
    dir: u64,
    target: View,
    expected: Option<u64>,
) -> WriteStatus {
    unsafe {
        let alloc = ptr::addr_of_mut!((*v.scratch()).attr_index);
        let Some(h) = r::load(v, dir, v.record()) else {
            return WriteStatus::Io;
        };
        let record = v.record();
        let Some(root) = idx::Root::locate(r::view(record), h) else {
            return WriteStatus::Invalid;
        };
        let mut pos = root.header_at + le32(r::view(record), root.header_at) as usize;
        let end = root.header_at + le32(r::view(record), root.header_at + 4) as usize;
        let mut next = None;
        while pos < end {
            if idx::entry_end(r::view(record), pos) {
                next = idx::entry_child(r::view(record), pos);
                break;
            }
            let order = names::compare(
                v.upcase.get(),
                target.get(),
                idx::entry_name(r::view(record), pos),
            );
            if order == 0 {
                if expected.is_some_and(|x| le64(r::view(record), pos) != x) {
                    return WriteStatus::Io;
                }
                return if let Some(child) = idx::entry_child(r::view(record), pos) {
                    interior(v, dir, true, 0, target, child)
                } else {
                    root_remove(v, dir, target)
                };
            }
            if order < 0 {
                next = idx::entry_child(r::view(record), pos);
                break;
            }
            pos += idx::entry_len(r::view(record), pos)
        }
        let Some(mut current) = next else {
            return WriteStatus::NotFound;
        };
        if r::collect(v, dir, 0xa0, View::of(&idx::I30), alloc) != Lookup::Found {
            return WriteStatus::Io;
        }
        for _ in 0..8 {
            if !idx::load(v, r::attr_runs(alloc), current) {
                return WriteStatus::Io;
            }
            let b = idx::block(v);
            let mut pos = 24 + le32(r::view(b), 24) as usize;
            let end = 24 + le32(r::view(b), 28) as usize;
            let mut next = None;
            while pos < end {
                if idx::entry_end(r::view(b), pos) {
                    next = idx::entry_child(r::view(b), pos);
                    break;
                }
                let order = names::compare(
                    v.upcase.get(),
                    target.get(),
                    idx::entry_name(r::view(b), pos),
                );
                if order == 0 {
                    if expected.is_some_and(|x| le64(r::view(b), pos) != x) {
                        return WriteStatus::Io;
                    }
                    return if let Some(child) = idx::entry_child(r::view(b), pos) {
                        interior(v, dir, false, current, target, child)
                    } else {
                        block_remove(v, dir, r::attr_runs(alloc), current, target, true)
                    };
                }
                if order < 0 {
                    next = idx::entry_child(r::view(b), pos);
                    break;
                }
                pos += idx::entry_len(r::view(b), pos)
            }
            let Some(child) = next else {
                return WriteStatus::NotFound;
            };
            current = child
        }
        WriteStatus::Io
    }
}
#[inline(never)]
unsafe fn root_remove(v: Volume, dir: u64, target: View) -> WriteStatus {
    unsafe {
        let Some(h0) = r::load(v, dir, v.record()) else {
            return WriteStatus::Io;
        };
        let record = v.record();
        let Some(root) = idx::Root::locate(r::view(record), h0) else {
            return WriteStatus::Invalid;
        };
        let mut pos = root.header_at + le32(r::view(record), root.header_at) as usize;
        let length = le32(r::view(record), root.header_at + 4);
        let end = root.header_at + length as usize;
        let mut found = 0;
        while pos < end {
            if idx::entry_end(r::view(record), pos) {
                break;
            }
            if names::compare(
                v.upcase.get(),
                idx::entry_name(r::view(record), pos),
                target.get(),
            ) == 0
            {
                found = idx::entry_len(r::view(record), pos);
                break;
            }
            pos += idx::entry_len(r::view(record), pos)
        }
        if found == 0 {
            return WriteStatus::NotFound;
        }
        r::mutable(record).copy_within(pos + found..end, pos);
        let mut h = h0;
        r::put32(
            r::mutable(record),
            root.header_at + 4,
            length - found as u32,
        );
        let old = le32(r::view(record), root.header_at + 8);
        r::put32(r::mutable(record), root.header_at + 8, old - found as u32);
        let old = le32(r::view(record), root.span.offset + 16);
        r::put32(
            r::mutable(record),
            root.span.offset + 16,
            old - found as u32,
        );
        let mut span = root.span;
        let new = span.length - found;
        let _ = edit::resize(r::mutable(record), &mut h, &mut span, new);
        result(r::store(v, dir, record))
    }
}
#[inline(never)]
unsafe fn block_remove(
    v: Volume,
    dir: u64,
    runs: io::Runs,
    vcn: u64,
    target: View,
    retire_empty: bool,
) -> WriteStatus {
    unsafe {
        if !idx::load(v, runs, vcn) {
            return WriteStatus::Io;
        }
        let b = full(v);
        let mut pos = 24 + le32(r::view(b), 24) as usize;
        let length = le32(r::view(b), 28);
        let end = 24 + length as usize;
        while pos < end {
            if idx::entry_end(r::view(b), pos) {
                return WriteStatus::NotFound;
            }
            if names::compare(
                v.upcase.get(),
                idx::entry_name(r::view(b), pos),
                target.get(),
            ) == 0
            {
                let n = idx::entry_len(r::view(b), pos);
                r::mutable(b).copy_within(pos + n..end, pos);
                r::put32(r::mutable(b), 28, length - n as u32);
                let empty = retire_empty && !idx::has_entries(r::view(idx::block(v)));
                if !idx::store(v, dir, runs, vcn) {
                    return WriteStatus::Io;
                }
                return if empty {
                    eliminate(v, dir, vcn)
                } else {
                    WriteStatus::Ok
                };
            }
            pos += idx::entry_len(r::view(b), pos)
        }
        WriteStatus::NotFound
    }
}
#[derive(Clone, Copy)]
struct Referrer {
    in_root: bool,
    node: u64,
    pos: usize,
    is_end: bool,
}
#[inline(never)]
unsafe fn referrer(v: Volume, dir: u64, runs: io::Runs, vcn: u64) -> Option<Referrer> {
    unsafe {
        {
            let h = r::load(v, dir, v.record())?;
            let record = v.record();
            let root = idx::Root::locate(r::view(record), h)?;
            let mut pos = root.header_at + le32(r::view(record), root.header_at) as usize;
            let end = root.header_at + le32(r::view(record), root.header_at + 4) as usize;
            while pos < end {
                if idx::entry_child(r::view(record), pos) == Some(vcn) {
                    return Some(Referrer {
                        in_root: true,
                        node: 0,
                        pos,
                        is_end: idx::entry_end(r::view(record), pos),
                    });
                }
                if idx::entry_end(r::view(record), pos) {
                    break;
                }
                pos += idx::entry_len(r::view(record), pos)
            }
        }
        let mut clusters = 0u64;
        for i in 0..runs.len {
            clusters = clusters.checked_add(runs.get(i).length_clusters)?
        }
        let bytes = clusters.checked_mul(u64::from(v.geometry().cluster_bytes))?;
        if v.index_block_bytes == 0 {
            return None;
        }
        let total = bytes / u64::from(v.index_block_bytes);
        for scan in 0..total {
            if scan == vcn || !idx::load(v, runs, scan) {
                continue;
            }
            let b = full(v);
            let mut pos = 24 + le32(r::view(b), 24) as usize;
            let end = 24 + le32(r::view(b), 28) as usize;
            while pos < end {
                if idx::entry_child(r::view(b), pos) == Some(vcn) {
                    return Some(Referrer {
                        in_root: false,
                        node: scan,
                        pos,
                        is_end: idx::entry_end(r::view(b), pos),
                    });
                }
                if idx::entry_end(r::view(b), pos) {
                    break;
                }
                pos += idx::entry_len(r::view(b), pos)
            }
        }
        None
    }
}
unsafe fn copy_plain(v: Volume, buffer: Buffer, pos: usize) -> bool {
    unsafe {
        let filename = le16(r::view(buffer), pos + 10) as usize;
        let len = (16 + filename + 7) & !7usize;
        if len > ENTRY_MAX {
            return false;
        }
        let out = insert::entry_a(v, len);
        ptr::write_bytes(out.data, 0, len);
        ptr::copy_nonoverlapping(buffer.data.add(pos), out.data, 16);
        ptr::copy_nonoverlapping(buffer.data.add(pos + 16), out.data.add(16), filename);
        r::put16(r::mutable(out), 8, len as u16);
        r::put16(r::mutable(out), 12, 0);
        true
    }
}
#[inline(never)]
pub(super) unsafe fn eliminate(v: Volume, dir: u64, first: u64) -> WriteStatus {
    unsafe {
        let mut vcn = first;
        for _ in 0..10 {
            let alloc = ptr::addr_of_mut!((*v.scratch()).attr_index);
            if r::collect(v, dir, 0xa0, View::of(&idx::I30), alloc) != Lookup::Found
                || !idx::load(v, r::attr_runs(alloc), vcn)
            {
                return WriteStatus::Io;
            }
            let b = idx::block(v);
            if idx::has_entries(r::view(b)) {
                return WriteStatus::Ok;
            }
            let bypass = idx::end_child(r::view(b));
            let Some(parent) = referrer(v, dir, r::attr_runs(alloc), vcn) else {
                return WriteStatus::Io;
            };
            if let Some(child) = bypass {
                if parent.in_root {
                    let Some(h) = r::load(v, dir, v.record()) else {
                        return WriteStatus::Io;
                    };
                    let record = v.record();
                    if idx::Root::locate(r::view(record), h).is_none() {
                        return WriteStatus::Invalid;
                    }
                    idx::set_child(r::mutable(record), parent.pos, child);
                    if !r::store(v, dir, record) {
                        return WriteStatus::Io;
                    }
                } else {
                    if !idx::load(v, r::attr_runs(alloc), parent.node) {
                        return WriteStatus::Io;
                    }
                    idx::set_child(r::mutable(full(v)), parent.pos, child);
                    if !idx::store(v, dir, r::attr_runs(alloc), parent.node) {
                        return WriteStatus::Io;
                    }
                }
                return result(allocation::free(v, dir, &[vcn]));
            }
            if !parent.is_end {
                if parent.in_root {
                    if r::load(v, dir, v.record()).is_none()
                        || !copy_plain(v, v.record(), parent.pos)
                    {
                        return WriteStatus::Io;
                    }
                } else if !idx::load(v, r::attr_runs(alloc), parent.node)
                    || !copy_plain(v, full(v), parent.pos)
                {
                    return WriteStatus::Io;
                }
                let plain =
                    insert::entry_a(v, idx::entry_len(r::view(insert::entry_a(v, ENTRY_MAX)), 0));
                let name = idx::entry_name(r::view(plain), 0);
                let len = name.len();
                if len > 512 {
                    return WriteStatus::Io;
                }
                let mut saved = [0u8; 512];
                saved[..len].copy_from_slice(name);
                let name = View::of(&saved[..len]);
                let status = if parent.in_root {
                    root_remove(v, dir, name)
                } else {
                    block_remove(v, dir, r::attr_runs(alloc), parent.node, name, false)
                };
                if status != WriteStatus::Ok {
                    return status;
                }
                if !allocation::free(v, dir, &[vcn]) {
                    return WriteStatus::Io;
                }
                let n = idx::entry_len(r::view(insert::entry_a(v, ENTRY_MAX)), 0);
                return insert::prepared(v, dir, n);
            }
            if parent.in_root {
                let Some(h) = r::load(v, dir, v.record()) else {
                    return WriteStatus::Io;
                };
                let record = v.record();
                let Some(root) = idx::Root::locate(r::view(record), h) else {
                    return WriteStatus::Invalid;
                };
                let at = root.header_at + le32(r::view(record), root.header_at) as usize;
                return if !idx::entry_end(r::view(record), at) {
                    rotate_root(v, dir, vcn)
                } else {
                    collapse(v, dir)
                };
            }
            if !idx::load(v, r::attr_runs(alloc), parent.node) {
                return WriteStatus::Io;
            }
            let b = idx::block(v);
            if idx::has_entries(r::view(b)) {
                return rotate_block(v, dir, parent.node, vcn);
            }
            let pos = 24 + le32(r::view(b), 24) as usize;
            if idx::entry_len(r::view(b), pos) < 24 {
                return WriteStatus::Io;
            }
            r::put16(r::mutable(full(v)), pos + 8, 16);
            r::put16(r::mutable(full(v)), pos + 12, 2);
            let old = le32(r::view(b), 28);
            r::put32(r::mutable(full(v)), 28, old - 8);
            *full(v).data.add(36) = 0;
            if !idx::store(v, dir, r::attr_runs(alloc), parent.node)
                || !allocation::free(v, dir, &[vcn])
            {
                return WriteStatus::Io;
            }
            vcn = parent.node;
        }
        WriteStatus::Io
    }
}
#[inline(never)]
unsafe fn rotate_root(v: Volume, dir: u64, leaf: u64) -> WriteStatus {
    unsafe {
        let Some(h0) = r::load(v, dir, v.record()) else {
            return WriteStatus::Io;
        };
        let record = v.record();
        let Some(root) = idx::Root::locate(r::view(record), h0) else {
            return WriteStatus::Invalid;
        };
        let mut pos = root.header_at + le32(r::view(record), root.header_at) as usize;
        let length = le32(r::view(record), root.header_at + 4);
        let end = root.header_at + length as usize;
        let mut last = None;
        while pos < end && !idx::entry_end(r::view(record), pos) {
            last = Some(pos);
            pos += idx::entry_len(r::view(record), pos)
        }
        let Some(q) = last else {
            return WriteStatus::Io;
        };
        let Some(child) = idx::entry_child(r::view(record), q) else {
            return WriteStatus::Io;
        };
        if !copy_plain(v, record, q) {
            return WriteStatus::Io;
        }
        let n = idx::entry_len(r::view(record), q);
        r::mutable(record).copy_within(q + n..end, q);
        let mut h = h0;
        let length = length - n as u32;
        r::put32(r::mutable(record), root.header_at + 4, length);
        r::put32(r::mutable(record), root.header_at + 8, length);
        let old = le32(r::view(record), root.span.offset + 16);
        r::put32(r::mutable(record), root.span.offset + 16, old - n as u32);
        let mut span = root.span;
        let len = span.length - n;
        let _ = edit::resize(r::mutable(record), &mut h, &mut span, len);
        idx::set_child(r::mutable(record), q, child);
        if !r::store(v, dir, record) || !allocation::free(v, dir, &[leaf]) {
            return WriteStatus::Io;
        }
        let len = idx::entry_len(r::view(insert::entry_a(v, ENTRY_MAX)), 0);
        insert::prepared(v, dir, len)
    }
}
#[inline(never)]
unsafe fn rotate_block(v: Volume, dir: u64, node: u64, leaf: u64) -> WriteStatus {
    unsafe {
        let alloc = ptr::addr_of_mut!((*v.scratch()).attr_index);
        if r::collect(v, dir, 0xa0, View::of(&idx::I30), alloc) != Lookup::Found
            || !idx::load(v, r::attr_runs(alloc), node)
        {
            return WriteStatus::Io;
        }
        let b = full(v);
        let mut pos = 24 + le32(r::view(b), 24) as usize;
        let length = le32(r::view(b), 28);
        let end = 24 + length as usize;
        let mut last = None;
        while pos < end && !idx::entry_end(r::view(b), pos) {
            last = Some(pos);
            pos += idx::entry_len(r::view(b), pos)
        }
        let Some(q) = last else {
            return WriteStatus::Io;
        };
        let Some(child) = idx::entry_child(r::view(b), q) else {
            return WriteStatus::Io;
        };
        if !copy_plain(v, b, q) {
            return WriteStatus::Io;
        }
        let n = idx::entry_len(r::view(b), q);
        r::mutable(b).copy_within(q + n..end, q);
        r::put32(r::mutable(b), 28, length - n as u32);
        idx::set_child(r::mutable(b), q, child);
        if !idx::store(v, dir, r::attr_runs(alloc), node) || !allocation::free(v, dir, &[leaf]) {
            return WriteStatus::Io;
        }
        let len = idx::entry_len(r::view(insert::entry_a(v, ENTRY_MAX)), 0);
        insert::prepared(v, dir, len)
    }
}
#[inline(never)]
unsafe fn collapse(v: Volume, dir: u64) -> WriteStatus {
    unsafe {
        let alloc = ptr::addr_of_mut!((*v.scratch()).attr_mgmt_b);
        let status = r::collect(v, dir, 0xa0, View::of(&idx::I30), alloc);
        if status == Lookup::Io {
            return WriteStatus::Io;
        }
        let Some(mut h) = r::load(v, dir, v.write_record()) else {
            return WriteStatus::Io;
        };
        let record = v.write_record();
        let mut value = [0u8; 0x30];
        r::put32(&mut value, 0, 0x30);
        r::put32(&mut value, 4, 1);
        r::put32(&mut value, 8, v.index_block_bytes);
        value[12] = 1;
        r::put32(&mut value, 16, 16);
        r::put32(&mut value, 20, 32);
        r::put32(&mut value, 24, 32);
        r::put16(&mut value, 40, 16);
        r::put16(&mut value, 44, 2);
        if !edit::resident(r::mutable(record), &mut h, 0x90, &idx::I30, &value) {
            return WriteStatus::Io;
        }
        let _ = edit::remove(r::mutable(record), 0xa0, &idx::I30);
        let _ = edit::remove(r::mutable(record), 0xb0, &idx::I30);
        if !r::store(v, dir, record) {
            return WriteStatus::Io;
        }
        if status == Lookup::Found && !(*alloc).resident && !clusters::free(v, r::attr_runs(alloc))
        {
            return WriteStatus::Io;
        }
        WriteStatus::Ok
    }
}
#[inline(never)]
unsafe fn interior(
    v: Volume,
    dir: u64,
    in_root: bool,
    node: u64,
    target: View,
    sub: u64,
) -> WriteStatus {
    unsafe {
        let alloc = ptr::addr_of_mut!((*v.scratch()).attr_index);
        if r::collect(v, dir, 0xa0, View::of(&idx::I30), alloc) != Lookup::Found {
            return WriteStatus::Io;
        }
        let mut chain = [0u64; 8];
        let mut count = 0;
        let mut deepest = None;
        let mut current = sub;
        for _ in 0..8 {
            if !idx::load(v, r::attr_runs(alloc), current) {
                return WriteStatus::Io;
            }
            let b = idx::block(v);
            if idx::has_entries(r::view(b)) {
                deepest = Some(current);
                count = 0
            } else {
                if count >= 8 {
                    return WriteStatus::Io;
                }
                chain[count] = current;
                count += 1
            }
            let Some(child) = idx::end_child(r::view(b)) else {
                break;
            };
            current = child
        }
        let Some(k) = deepest else {
            let status = if in_root {
                root_remove(v, dir, target)
            } else {
                block_remove(v, dir, r::attr_runs(alloc), node, target, true)
            };
            if status != WriteStatus::Ok {
                return status;
            }
            return result(allocation::free(v, dir, &chain[..count]));
        };
        if !idx::load(v, r::attr_runs(alloc), k) {
            return WriteStatus::Io;
        }
        let b = full(v);
        let mut pos = 24 + le32(r::view(b), 24) as usize;
        let mut last = None;
        while !idx::entry_end(r::view(b), pos) {
            last = Some(pos);
            pos += idx::entry_len(r::view(b), pos)
        }
        let Some(q) = last else {
            return WriteStatus::Io;
        };
        let n = idx::entry_len(r::view(b), q);
        let child = idx::entry_child(r::view(b), q);
        let filename = le16(r::view(b), q + 10) as usize;
        let plain = (16 + filename + 7) & !7usize;
        if plain > ENTRY_MAX {
            return WriteStatus::Io;
        }
        let qb = Buffer {
            data: ptr::addr_of_mut!((*v.scratch()).entry_b).cast::<u8>(),
            len: plain,
        };
        ptr::write_bytes(qb.data, 0, plain);
        ptr::copy_nonoverlapping(b.data.add(q), qb.data, 16);
        ptr::copy_nonoverlapping(b.data.add(q + 16), qb.data.add(16), filename);
        r::put16(r::mutable(qb), 8, plain as u16);
        r::put16(r::mutable(qb), 12, 0);
        let leaf = idx::end_child(r::view(idx::block(v))).is_none();
        let length = le32(r::view(b), 28);
        let end = 24 + length as usize;
        r::mutable(b).copy_within(q + n..end, q);
        r::put32(r::mutable(b), 28, length - n as u32);
        if !leaf {
            if idx::entry_child(r::view(b), q).is_none() {
                return WriteStatus::Io;
            }
            let Some(child) = child else {
                return WriteStatus::Io;
            };
            idx::set_child(r::mutable(b), q, child)
        }
        let empty = !idx::has_entries(r::view(idx::block(v)));
        if !idx::store(v, dir, r::attr_runs(alloc), k) {
            return WriteStatus::Io;
        }
        if !leaf && !allocation::free(v, dir, &chain[..count]) {
            return WriteStatus::Io;
        }
        let status = replace_interior(v, dir, in_root, node, target, plain);
        if status != WriteStatus::Ok {
            return status;
        }
        if empty {
            eliminate(v, dir, k)
        } else {
            WriteStatus::Ok
        }
    }
}
#[inline(never)]
unsafe fn replace_interior(
    v: Volume,
    dir: u64,
    in_root: bool,
    vcn: u64,
    target: View,
    plain: usize,
) -> WriteStatus {
    unsafe {
        if in_root {
            let Some(h0) = r::load(v, dir, v.record()) else {
                return WriteStatus::Io;
            };
            let record = v.record();
            let Some(root) = idx::Root::locate(r::view(record), h0) else {
                return WriteStatus::Invalid;
            };
            let mut pos = root.header_at + le32(r::view(record), root.header_at) as usize;
            let length = le32(r::view(record), root.header_at + 4);
            let end = root.header_at + length as usize;
            while pos < end && !idx::entry_end(r::view(record), pos) {
                if names::compare(
                    v.upcase.get(),
                    idx::entry_name(r::view(record), pos),
                    target.get(),
                ) == 0
                {
                    let old = idx::entry_len(r::view(record), pos);
                    let Some(child) = idx::entry_child(r::view(record), pos) else {
                        return WriteStatus::Io;
                    };
                    let new = plain + 8;
                    let mut h = h0;
                    let mut span = root.span;
                    if new > old {
                        let size = span.length + (new - old);
                        if !edit::resize(r::mutable(record), &mut h, &mut span, size) {
                            let status = insert::push_down(v, dir);
                            if status != WriteStatus::Ok {
                                return status;
                            }
                            let Some(next) = insert::root_child(v, dir) else {
                                return WriteStatus::Io;
                            };
                            return replace_interior(v, dir, false, next, target, plain);
                        }
                    }
                    r::mutable(record).copy_within(pos + old..end, pos + new);
                    ptr::copy_nonoverlapping(
                        ptr::addr_of!((*v.scratch()).entry_b).cast::<u8>(),
                        record.data.add(pos),
                        plain,
                    );
                    r::put16(r::mutable(record), pos + 8, new as u16);
                    r::put16(r::mutable(record), pos + 12, 1);
                    ptr::write_bytes(record.data.add(pos + plain), 0, new - plain);
                    r::put64(r::mutable(record), pos + new - 8, child);
                    let delta = new as i64 - old as i64;
                    let size = u32::try_from(i64::from(length) + delta)
                        .expect("NTFS replacement index size");
                    r::put32(r::mutable(record), root.header_at + 4, size);
                    r::put32(r::mutable(record), root.header_at + 8, size);
                    let value = u32::try_from(
                        i64::from(le32(r::view(record), root.span.offset + 16)) + delta,
                    )
                    .expect("NTFS root value size");
                    r::put32(r::mutable(record), root.span.offset + 16, value);
                    if new < old {
                        let size = span.length - (old - new);
                        let _ = edit::resize(r::mutable(record), &mut h, &mut span, size);
                    }
                    return result(r::store(v, dir, record));
                }
                pos += idx::entry_len(r::view(record), pos)
            }
            return WriteStatus::NotFound;
        }
        let alloc = ptr::addr_of_mut!((*v.scratch()).attr_index);
        if r::collect(v, dir, 0xa0, View::of(&idx::I30), alloc) != Lookup::Found
            || !idx::load(v, r::attr_runs(alloc), vcn)
        {
            return WriteStatus::Io;
        }
        let b = full(v);
        let mut pos = 24 + le32(r::view(b), 24) as usize;
        let length = le32(r::view(b), 28);
        let end = 24 + length as usize;
        while pos < end && !idx::entry_end(r::view(b), pos) {
            if names::compare(
                v.upcase.get(),
                idx::entry_name(r::view(b), pos),
                target.get(),
            ) == 0
            {
                let old = idx::entry_len(r::view(b), pos);
                let Some(child) = idx::entry_child(r::view(b), pos) else {
                    return WriteStatus::Io;
                };
                let new = plain + 8;
                r::mutable(b).copy_within(pos + old..end, pos + new);
                ptr::copy_nonoverlapping(
                    ptr::addr_of!((*v.scratch()).entry_b).cast::<u8>(),
                    b.data.add(pos),
                    plain,
                );
                r::put16(r::mutable(b), pos + 8, new as u16);
                r::put16(r::mutable(b), pos + 12, 1);
                ptr::write_bytes(b.data.add(pos + plain), 0, new - plain);
                r::put64(r::mutable(b), pos + new - 8, child);
                r::put32(
                    r::mutable(b),
                    28,
                    u32::try_from(i64::from(length) + new as i64 - old as i64)
                        .expect("NTFS index replacement size"),
                );
                if insert::fits(v) {
                    return result(idx::store(v, dir, r::attr_runs(alloc), vcn));
                }
                let Some(fresh) = allocation::allocate(v, dir) else {
                    return WriteStatus::NoSpace;
                };
                if r::collect(v, dir, 0xa0, View::of(&idx::I30), alloc) != Lookup::Found
                    || !insert::split(v, dir, r::attr_runs(alloc), vcn, fresh)
                {
                    return WriteStatus::Io;
                }
                let n = idx::entry_len(r::view(insert::entry_a(v, ENTRY_MAX)), 0);
                return insert::prepared(v, dir, n);
            }
            pos += idx::entry_len(r::view(b), pos)
        }
        WriteStatus::NotFound
    }
}
