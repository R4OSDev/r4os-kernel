//! Original plan-before-mark bitmap allocation and uncertain-prefix rollback.
use super::{
    ntfs_format_types::*, ntfs_volume_io as io, ntfs_volume_records as r, ntfs_volume_types::*,
};
use core::ptr;
#[derive(Clone, Copy)]
pub struct RunBuffer {
    pub data: *mut Run,
    pub len: usize,
}
impl RunBuffer {
    pub fn prefix(self, n: usize) -> io::Runs {
        assert!(n <= self.len);
        io::Runs {
            data: self.data,
            len: n,
        }
    }
}
#[inline(never)]
pub(super) unsafe fn allocate(v: Volume, count: u64, out: RunBuffer) -> AllocationResult {
    unsafe {
        let bitmap = ptr::addr_of_mut!((*v.scratch()).attr_cluster);
        if r::collect(v, 6, 0x80, View::default(), bitmap) != Lookup::Found || (*bitmap).resident {
            return AllocationResult::error(WriteStatus::Io);
        }
        if count == 0 {
            return AllocationResult::error(WriteStatus::Ok);
        }
        let total = v.geometry().total_clusters();
        let hint = (*v.scratch()).cluster_search_start;
        let start = if hint < total { hint } else { 0 };
        let mut planned = plan(v, bitmap, total, start, count, out);
        if planned.status == WriteStatus::RecordFull && start != 0 {
            planned = plan(v, bitmap, total, 0, count, out)
        }
        if planned.status != WriteStatus::Ok {
            return planned;
        }
        for i in 0..planned.produced {
            let run = out.data.add(i).read();
            assert!(run.has_lcn);
            if !set_range(v, bitmap, run.lcn, run.length_clusters, true) {
                return AllocationResult::error(if rollback(v, bitmap, out.prefix(i + 1)) {
                    WriteStatus::Io
                } else {
                    WriteStatus::CleanupFailed
                });
            }
        }
        let last = out.data.add(planned.produced - 1).read();
        assert!(last.has_lcn);
        let next = last.lcn + last.length_clusters;
        (*v.scratch()).cluster_search_start = if next < total { next } else { 0 };
        planned
    }
}
unsafe fn plan(
    v: Volume,
    bitmap: *const AttrScratch,
    total: u64,
    start: u64,
    count: u64,
    out: RunBuffer,
) -> AllocationResult {
    unsafe {
        let mut needed = count;
        let mut from = start;
        let mut end = total;
        let mut wrapped = start == 0;
        let mut produced = 0;
        while needed != 0 {
            if produced == out.len {
                return AllocationResult::error(WriteStatus::RecordFull);
            }
            let found = find(v, bitmap, end, from, needed);
            if found.status == WriteStatus::NoSpace && !wrapped {
                from = 0;
                end = start;
                wrapped = true;
                continue;
            }
            if found.status != WriteStatus::Ok {
                return AllocationResult::error(found.status);
            }
            out.data.add(produced).write(found.run);
            produced += 1;
            needed -= found.run.length_clusters;
            assert!(found.run.has_lcn);
            from = found.run.lcn + found.run.length_clusters
        }
        AllocationResult {
            status: WriteStatus::Ok,
            produced,
        }
    }
}
#[inline(never)]
pub(super) unsafe fn rollback(v: Volume, bitmap: *const AttrScratch, runs: io::Runs) -> bool {
    unsafe {
        let mut ok = true;
        for i in 0..runs.len {
            let run = runs.get(i);
            if run.has_lcn && !set_range(v, bitmap, run.lcn, run.length_clusters, false) {
                ok = false
            }
        }
        ok
    }
}
#[inline(never)]
pub(super) unsafe fn free(v: Volume, runs: io::Runs) -> bool {
    unsafe {
        if !io::runs_valid(v.io, runs) {
            return false;
        }
        let bitmap = ptr::addr_of_mut!((*v.scratch()).attr_cluster);
        if r::collect(v, 6, 0x80, View::default(), bitmap) != Lookup::Found {
            return false;
        }
        for i in 0..runs.len {
            let run = runs.get(i);
            if !run.has_lcn {
                continue;
            }
            if !set_range(v, bitmap, run.lcn, run.length_clusters, false) {
                return false;
            }
            (*v.scratch()).cluster_search_start = (*v.scratch()).cluster_search_start.min(run.lcn)
        }
        true
    }
}
#[inline(never)]
unsafe fn find(v: Volume, bitmap: *const AttrScratch, total: u64, from: u64, max: u64) -> FindRun {
    unsafe {
        let mut sector = [0u8; 512];
        let mut start = None;
        let mut n = 0;
        let mut cluster = from;
        let mut loaded = u64::MAX;
        while cluster < total {
            if v.io.work_counters {
                (*v.scratch()).work.cluster_bitmap_probes += 1
            }
            let byte = cluster / 8;
            let sector_index = byte / 512;
            if sector_index != loaded {
                let Some(offset) = sector_index.checked_mul(512) else {
                    return FindRun::error(WriteStatus::Io);
                };
                if !io::runs(
                    v.io,
                    r::attr_runs(bitmap),
                    offset,
                    Buffer {
                        data: sector.as_mut_ptr(),
                        len: 512,
                    },
                    None,
                    ptr::null_mut(),
                ) {
                    return FindRun::error(WriteStatus::Io);
                }
                loaded = sector_index
            }
            let bit = (sector[(byte % 512) as usize] >> (cluster % 8)) & 1;
            if bit == 0 {
                if start.is_none() {
                    start = Some(cluster)
                }
                n += 1;
                if n == max {
                    break;
                }
            } else if start.is_some() {
                break;
            }
            cluster += 1
        }
        let Some(start) = start else {
            return FindRun::error(WriteStatus::NoSpace);
        };
        if n == 0 {
            return FindRun::error(WriteStatus::NoSpace);
        }
        FindRun {
            status: WriteStatus::Ok,
            run: Run {
                length_clusters: n,
                has_lcn: true,
                lcn: start,
            },
        }
    }
}
#[inline(never)]
pub(super) unsafe fn set_range(
    v: Volume,
    bitmap: *const AttrScratch,
    lcn: u64,
    count: u64,
    set: bool,
) -> bool {
    unsafe {
        if count == 0 {
            return true;
        }
        let total = v.geometry().total_clusters();
        if lcn >= total || count > total - lcn {
            return false;
        }
        let Some(end) = lcn.checked_add(count) else {
            return false;
        };
        let mut sector = [0u8; 512];
        let mut cluster = lcn;
        while cluster < end {
            let index = cluster / 8 / 512;
            let Some(offset) = index.checked_mul(512) else {
                return false;
            };
            let buffer = Buffer {
                data: sector.as_mut_ptr(),
                len: 512,
            };
            if !io::runs(
                v.io,
                r::attr_runs(bitmap),
                offset,
                buffer,
                None,
                ptr::null_mut(),
            ) {
                return false;
            }
            let mut changed = 0u64;
            while cluster < end && cluster / 8 / 512 == index {
                let byte = (cluster / 8 % 512) as usize;
                let mask = 1u8 << (cluster % 8);
                if (sector[byte] & mask != 0) != set {
                    changed += 1
                }
                if set {
                    sector[byte] |= mask
                } else {
                    sector[byte] &= !mask
                }
                cluster += 1
            }
            let previous = if v.cache().is_null() {
                None
            } else {
                (*v.cache()).free_clusters(total)
            };
            if !io::runs(
                v.io,
                r::attr_runs(bitmap),
                offset,
                Buffer {
                    data: sector.as_mut_ptr(),
                    len: 512,
                },
                Some(Mutation::Attribute {
                    record: 6,
                    kind: 0x80,
                }),
                ptr::null_mut(),
            ) {
                return false;
            }
            if let Some(free) = previous {
                if set {
                    if changed <= free {
                        (*v.cache()).store_free_clusters(total, free - changed)
                    }
                } else if changed <= total - free {
                    (*v.cache()).store_free_clusters(total, free + changed)
                }
            }
        }
        true
    }
}
#[inline(never)]
pub(super) unsafe fn free_count(v: Volume) -> Option<u64> {
    unsafe {
        let total = v.geometry().total_clusters();
        if !v.cache().is_null()
            && let Some(free) = (*v.cache()).free_clusters(total)
        {
            return Some(free);
        }
        let bitmap = ptr::addr_of_mut!((*v.scratch()).attr_cluster);
        if r::collect(v, 6, 0x80, View::default(), bitmap) != Lookup::Found || (*bitmap).resident {
            return None;
        }
        let needed = total / 8 + u64::from(!total.is_multiple_of(8));
        if needed > (*bitmap).data_size || needed > (*bitmap).initialized_size {
            return None;
        }
        let data = ptr::addr_of_mut!((*v.scratch()).comp_in).cast::<u8>();
        let mut free = 0;
        let mut cluster = 0;
        while cluster < total {
            let bits = (total - cluster).min(65536 * 8);
            let bytes = (bits / 8 + u64::from(!bits.is_multiple_of(8))) as usize;
            if !io::runs(
                v.io,
                r::attr_runs(bitmap),
                cluster / 8,
                Buffer { data, len: bytes },
                None,
                ptr::null_mut(),
            ) {
                return None;
            }
            free += count_bits(core::slice::from_raw_parts(data, bytes), bits);
            cluster += bits
        }
        if !v.cache().is_null() {
            (*v.cache()).store_free_clusters(total, free)
        }
        Some(free)
    }
}
pub fn count_bits(bytes: &[u8], bits: u64) -> u64 {
    let full = (bits / 8) as usize;
    let mut used = 0u64;
    let mut i = 0;
    while full - i >= 8 {
        used += u64::from(
            u64::from_ne_bytes(bytes[i..i + 8].try_into().expect("NTFS bitmap word")).count_ones(),
        );
        i += 8
    }
    while i < full {
        used += u64::from(bytes[i].count_ones());
        i += 1
    }
    let tail = (bits % 8) as u8;
    if tail != 0 {
        used += u64::from((bytes[full] & ((1u8 << tail) - 1)).count_ones())
    }
    bits - used
}
