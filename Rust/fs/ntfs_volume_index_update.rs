//! Original duplicate filename/index size updates and diagnostic phases.
use super::{
    ntfs_format_names as names, ntfs_format_types::*, ntfs_volume_index_io as idx,
    ntfs_volume_names as diag, ntfs_volume_records as r, ntfs_volume_types::*,
};
use core::ptr;
pub fn patch(bytes: &mut [u8], pos: usize, allocation: u64, data: u64, mtime: u64) {
    let v = pos + 16;
    if mtime != 0 {
        r::put64(bytes, v + 16, mtime);
        r::put64(bytes, v + 24, mtime)
    }
    r::put64(bytes, v + 40, allocation);
    r::put64(bytes, v + 48, data)
}
#[inline(never)]
pub(super) unsafe fn update(
    v: Volume,
    dir: u64,
    name: View,
    allocation: u64,
    data: u64,
    mtime: u64,
) -> bool {
    unsafe {
        diag::append_at(171);
        let output = ptr::addr_of_mut!((*v.scratch()).name_utf16).cast::<u8>();
        let Some(n) = names::to_utf16(name.get(), core::slice::from_raw_parts_mut(output, 510))
        else {
            return false;
        };
        let target = View {
            data: output,
            len: n,
        };
        diag::append_at(172);
        let Some(h) = r::load(v, dir, v.record()) else {
            return false;
        };
        let record = v.record();
        diag::append_at(173);
        let Some(root) = idx::Root::locate(r::view(record), h) else {
            return false;
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
                patch(r::mutable(record), pos, allocation, data, mtime);
                diag::append_at(174);
                return r::store(v, dir, record);
            }
            if order < 0 {
                next = idx::entry_child(r::view(record), pos);
                break;
            }
            pos += idx::entry_len(r::view(record), pos)
        }
        diag::append_at(175);
        let Some(mut current) = next else {
            return false;
        };
        let alloc = ptr::addr_of_mut!((*v.scratch()).attr_index);
        diag::append_at(176);
        if r::collect(v, dir, 0xa0, View::of(&idx::I30), alloc) != Lookup::Found {
            return false;
        }
        for _ in 0..8 {
            diag::append_at(177);
            if !idx::load(v, r::attr_runs(alloc), current) {
                return false;
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
                    patch(r::mutable(b), pos, allocation, data, mtime);
                    diag::append_at(178);
                    return idx::store(v, dir, r::attr_runs(alloc), current);
                }
                if order < 0 {
                    next = idx::entry_child(r::view(b), pos);
                    break;
                }
                pos += idx::entry_len(r::view(b), pos)
            }
            diag::append_at(179);
            let Some(child) = next else { return false };
            current = child
        }
        diag::append_at(180);
        false
    }
}
