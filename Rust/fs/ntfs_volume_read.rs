//! Original resident/sparse/initialized/compressed byte reads.
//! Caller-owned attribute/runlist/scratch memory stays raw across I/O.
use super::{
    ntfs_format_lznt1,
    ntfs_format_types::{Buffer, Run},
    ntfs_volume_io as io,
    ntfs_volume_types::*,
};
use core::ptr;
unsafe fn run_span(attr: *const AttrScratch) -> io::Runs {
    unsafe {
        assert!((*attr).count <= 256);
        io::Runs {
            data: ptr::addr_of!((*attr).runs).cast::<Run>(),
            len: (*attr).count,
        }
    }
}
fn part(out: Buffer, start: usize, len: usize) -> Buffer {
    assert!(start <= out.len && len <= out.len - start);
    Buffer {
        data: out.data.wrapping_add(start),
        len,
    }
}
pub(super) unsafe fn attribute(
    v: io::Io,
    attr: *const AttrScratch,
    offset: usize,
    out: Buffer,
) -> Option<usize> {
    unsafe {
        if (*attr).flags & 0x4000 != 0 {
            return None;
        }
        let offset64 = offset as u64;
        if offset64 >= (*attr).data_size {
            return Some(0);
        }
        let want = (out.len as u64).min((*attr).data_size - offset64) as usize;
        if (*attr).resident {
            assert!(offset <= 1024 && want <= 1024 - offset);
            ptr::copy_nonoverlapping(
                ptr::addr_of!((*attr).resident_copy)
                    .cast::<u8>()
                    .add(offset),
                out.data,
                want,
            );
            return Some(want);
        }
        let initialized =
            (want as u64).min((*attr).initialized_size.saturating_sub(offset64)) as usize;
        // Compression may decode a whole physical unit, but only initialized
        // logical bytes belong to the caller. Do not read the zero-only tail.
        if initialized != 0 {
            let prefix = part(out, 0, initialized);
            let ok = if (*attr).flags & 1 != 0 {
                compressed(v, attr, offset64, prefix)
            } else {
                io::runs(v, run_span(attr), offset64, prefix, None, ptr::null_mut())
            };
            if !ok {
                return None;
            }
        }
        if initialized < want {
            ptr::write_bytes(out.data.add(initialized), 0, want - initialized);
            if v.work_counters {
                (*v.scratch).work.read_zero_bytes += (want - initialized) as u64
            }
        }
        Some(want)
    }
}
pub(super) unsafe fn mapped_in_unit(
    attr: *const AttrScratch,
    start: u64,
    count: u64,
) -> Option<u64> {
    unsafe {
        let end = start.checked_add(count)?;
        let mut mapped = 0u64;
        let mut vcn = 0u64;
        let runs = run_span(attr);
        for i in 0..runs.len {
            let r = runs.get(i);
            let re = vcn.checked_add(r.length_clusters)?;
            let lo = vcn.max(start);
            let hi = re.min(end);
            if hi > lo && r.has_lcn {
                mapped = mapped.checked_add(hi - lo)?
            }
            vcn = re;
            if vcn >= end {
                break;
            }
        }
        Some(mapped)
    }
}
pub(super) unsafe fn compressed(
    v: io::Io,
    attr: *const AttrScratch,
    offset: u64,
    out: Buffer,
) -> bool {
    unsafe {
        let shift = if (*attr).compression_unit == 0 {
            4
        } else {
            (*attr).compression_unit
        };
        if shift >= 64 {
            return false;
        }
        let clusters = 1u64 << shift;
        let Some(unit_bytes) = clusters.checked_mul(u64::from(v.geometry.cluster_bytes)) else {
            return false;
        };
        if unit_bytes == 0 || unit_bytes > 65536 {
            return false;
        }
        let mut pos = offset;
        let mut out_pos = 0;
        while out_pos < out.len {
            let unit_index = pos / unit_bytes;
            let inside = (pos % unit_bytes) as usize;
            let Some(start) = unit_index.checked_mul(clusters) else {
                return false;
            };
            let Some(mapped) = mapped_in_unit(attr, start, clusters) else {
                return false;
            };
            let take = (out.len - out_pos).min(unit_bytes as usize - inside);
            let Some(byte_offset) = unit_index.checked_mul(unit_bytes) else {
                return false;
            };
            let dest = part(out, out_pos, take);
            if mapped == clusters {
                let Some(read_offset) = byte_offset.checked_add(inside as u64) else {
                    return false;
                };
                if !io::runs(v, run_span(attr), read_offset, dest, None, ptr::null_mut()) {
                    return false;
                }
            } else if mapped == 0 {
                ptr::write_bytes(dest.data, 0, dest.len)
            } else {
                let Some(bytes) = mapped.checked_mul(u64::from(v.geometry.cluster_bytes)) else {
                    return false;
                };
                if bytes > 65536 {
                    return false;
                }
                let input = v.compressed_input(bytes as usize);
                if !io::runs(v, run_span(attr), byte_offset, input, None, ptr::null_mut()) {
                    return false;
                }
                let decoded = v.compressed_output(unit_bytes as usize);
                let Some(n) = ntfs_format_lznt1::decompress(
                    core::slice::from_raw_parts(input.data, input.len),
                    core::slice::from_raw_parts_mut(decoded.data, decoded.len),
                ) else {
                    return false;
                };
                if n < decoded.len {
                    ptr::write_bytes(decoded.data.add(n), 0, decoded.len - n)
                }
                ptr::copy_nonoverlapping(decoded.data.add(inside), dest.data, take)
            }
            out_pos += take;
            pos += take as u64;
        }
        true
    }
}
