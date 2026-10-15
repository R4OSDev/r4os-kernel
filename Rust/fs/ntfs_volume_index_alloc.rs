//! Original I30 allocation growth and exact resident bitmap ownership.
use super::{
    ntfs_format_runs as pairs, ntfs_format_types::*, ntfs_volume_clusters as c,
    ntfs_volume_edit as edit, ntfs_volume_index_io as idx, ntfs_volume_io as io,
    ntfs_volume_records as r, ntfs_volume_types::*,
};
use core::ptr;
#[inline(never)]
#[allow(clippy::manual_div_ceil)] // Preserve original checked-add geometry boundary.
pub(super) unsafe fn allocate(v: Volume, dir: u64) -> Option<u64> {
    unsafe {
        if v.geometry().cluster_bytes != v.index_block_bytes {
            return None;
        }
        let alloc = ptr::addr_of_mut!((*v.scratch()).attr_mgmt_a);
        let status = r::collect(v, dir, 0xa0, View::of(&idx::I30), alloc);
        if status == Lookup::Io {
            return None;
        }
        if status != Lookup::Found {
            let mut run = [Run::default(); 1];
            let result = c::allocate(
                v,
                1,
                c::RunBuffer {
                    data: run.as_mut_ptr(),
                    len: 1,
                },
            );
            if result.status != WriteStatus::Ok || result.produced != 1 {
                return None;
            }
            r::load(v, dir, v.record())?;
            let record = v.record();
            let mut bytes = [0u8; 0x60];
            r::put32(&mut bytes, 0, 0xa0);
            bytes[8] = 1;
            bytes[9] = 4;
            r::put16(&mut bytes, 10, 0x40);
            r::put64(&mut bytes, 0x10, 0);
            r::put64(&mut bytes, 0x18, 0);
            r::put16(&mut bytes, 0x20, 0x48);
            for o in [0x28, 0x30, 0x38] {
                r::put64(&mut bytes, o, u64::from(v.index_block_bytes))
            }
            bytes[0x40..0x48].copy_from_slice(&idx::I30);
            assert!(run[0].has_lcn);
            let mut n = pairs::encode(
                &mut bytes[0x48..],
                1,
                Some(i64::try_from(run[0].lcn).expect("NTFS LCN delta")),
            )?;
            bytes[0x48 + n] = 0;
            n += 1;
            let len = (0x48 + n + 7) & !7usize;
            r::put32(&mut bytes, 4, len as u32);
            if !edit::insert(r::mutable(record), &bytes[..len]) {
                if !c::free(
                    v,
                    io::Runs {
                        data: run.as_ptr(),
                        len: 1,
                    },
                ) {
                    return None;
                }
                return None;
            }
            let mut bitmap = [0u8; 0x28];
            r::put32(&mut bitmap, 0, 0xb0);
            r::put32(&mut bitmap, 4, 0x28);
            bitmap[9] = 4;
            r::put16(&mut bitmap, 10, 0x18);
            r::put32(&mut bitmap, 0x10, 8);
            r::put16(&mut bitmap, 0x14, 0x20);
            bitmap[0x18..0x20].copy_from_slice(&idx::I30);
            bitmap[0x20] = 1;
            if !edit::insert(r::mutable(record), &bitmap) {
                if !c::free(
                    v,
                    io::Runs {
                        data: run.as_ptr(),
                        len: 1,
                    },
                ) {
                    return None;
                }
                return None;
            }
            if !r::store(v, dir, record) {
                return None;
            }
            return Some(0);
        }
        let bitmap = ptr::addr_of_mut!((*v.scratch()).attr_mgmt_b);
        if r::collect(v, dir, 0xb0, View::of(&idx::I30), bitmap) != Lookup::Found
            || !(*bitmap).resident
        {
            return None;
        }
        let capacity = (*alloc).alloc_size / u64::from(v.index_block_bytes);
        let mut vcn = 0;
        while vcn < capacity {
            let byte = (vcn / 8) as usize;
            if byte >= (*bitmap).resident_len
                || ((*bitmap).resident_copy[byte] >> (vcn % 8)) & 1 == 0
            {
                break;
            }
            vcn += 1
        }
        let mut run = [Run::default(); 1];
        let mut grew = false;
        if vcn == capacity {
            if (*alloc).count >= 256 {
                return None;
            }
            let result = c::allocate(
                v,
                1,
                c::RunBuffer {
                    data: run.as_mut_ptr(),
                    len: 1,
                },
            );
            if result.status != WriteStatus::Ok || result.produced != 1 {
                return None;
            }
            let count = (*alloc).count;
            let adjacent = if count > 0 && (*alloc).runs[count - 1].has_lcn && run[0].has_lcn {
                (*alloc).runs[count - 1]
                    .lcn
                    .checked_add((*alloc).runs[count - 1].length_clusters)?
                    == run[0].lcn
            } else {
                false
            };
            if adjacent {
                (*alloc).runs[count - 1].length_clusters =
                    (*alloc).runs[count - 1].length_clusters.checked_add(1)?
            } else {
                (*alloc).runs[count] = run[0];
                (*alloc).count += 1
            }
            grew = true
        }
        let mut h = r::load(v, dir, v.record())?;
        let record = v.record();
        if grew {
            if !io::runs_valid(v.io, r::attr_runs(alloc)) {
                return None;
            }
            let bytes = (*alloc)
                .alloc_size
                .checked_add(u64::from(v.index_block_bytes))?;
            if !edit::nonresident(
                r::mutable(record),
                &mut h,
                0xa0,
                &idx::I30,
                core::slice::from_raw_parts(
                    ptr::addr_of!((*alloc).runs).cast::<Run>(),
                    (*alloc).count,
                ),
                bytes,
                bytes,
                bytes,
            ) {
                if !c::free(
                    v,
                    io::Runs {
                        data: run.as_ptr(),
                        len: 1,
                    },
                ) {
                    return None;
                }
                return None;
            }
        }
        let mut value = [0u8; 256];
        let mut len = (*bitmap).resident_len;
        if len > 256 {
            return None;
        }
        ptr::copy_nonoverlapping(
            ptr::addr_of!((*bitmap).resident_copy).cast::<u8>(),
            value.as_mut_ptr(),
            len,
        );
        let need = ((usize::try_from(vcn).expect("NTFS index VCN") / 8 + 1 + 7) / 8) * 8;
        if need > 256 {
            return None;
        }
        len = len.max(need);
        value[(vcn / 8) as usize] |= 1u8 << (vcn % 8);
        if !edit::resident(r::mutable(record), &mut h, 0xb0, &idx::I30, &value[..len])
            || !r::store(v, dir, record)
        {
            return None;
        }
        Some(vcn)
    }
}
#[inline(never)]
pub(super) unsafe fn free(v: Volume, dir: u64, vcns: &[u64]) -> bool {
    unsafe {
        if vcns.is_empty() {
            return true;
        }
        let bitmap = ptr::addr_of_mut!((*v.scratch()).attr_mgmt_a);
        if r::collect(v, dir, 0xb0, View::of(&idx::I30), bitmap) != Lookup::Found
            || !(*bitmap).resident
        {
            return false;
        }
        let mut value = [0u8; 256];
        let len = (*bitmap).resident_len;
        if len > 256 {
            return false;
        }
        ptr::copy_nonoverlapping(
            ptr::addr_of!((*bitmap).resident_copy).cast::<u8>(),
            value.as_mut_ptr(),
            len,
        );
        for vcn in vcns {
            let byte = (vcn / 8) as usize;
            if byte >= len {
                return false;
            }
            value[byte] &= !(1u8 << (vcn % 8))
        }
        let Some(mut h) = r::load(v, dir, v.record()) else {
            return false;
        };
        let record = v.record();
        if !edit::resident(r::mutable(record), &mut h, 0xb0, &idx::I30, &value[..len]) {
            return false;
        }
        r::store(v, dir, record)
    }
}
