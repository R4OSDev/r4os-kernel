//! Original range-checked, transport-bounded NTFS extent I/O.
//! Resident scratch and runlist addresses remain raw across device callbacks.
use super::{
    ntfs_format_types::{Buffer, Run},
    ntfs_volume_cache::Cache,
    ntfs_volume_types::*,
};
use core::ptr;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Device {
    pub context: *mut core::ffi::c_void,
    pub read: unsafe extern "C" fn(*mut core::ffi::c_void, u64, u32, *mut u8, usize) -> bool,
    pub write: unsafe extern "C" fn(*mut core::ffi::c_void, u64, u32, *const u8, usize) -> bool,
    pub flush: unsafe extern "C" fn(*mut core::ffi::c_void) -> bool,
    pub max_transfer_sectors: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Io {
    pub device: Device,
    pub geometry: Geometry,
    pub scratch: *mut Scratch,
    pub cache: *mut Cache,
    pub work_counters: bool,
}
#[derive(Clone, Copy)]
pub struct Runs {
    pub data: *const Run,
    pub len: usize,
}
impl Runs {
    pub(super) unsafe fn get(self, i: usize) -> Run {
        assert!(i < self.len);
        unsafe { self.data.add(i).read() }
    }
}
pub(super) unsafe fn read(v: Io, lba: u64, count: u32, out: Buffer) -> bool {
    if !v.geometry.sector_io_valid(lba, count, out.len) {
        return false;
    }
    unsafe { (v.device.read)(v.device.context, lba, count, out.data, out.len) }
}
pub(super) unsafe fn write(v: Io, lba: u64, count: u32, data: Buffer, kind: Mutation) -> bool {
    unsafe {
        if (*v.scratch).has_mirror_bootstrap_lcn
            || !v.geometry.sector_io_valid(lba, count, data.len)
        {
            return false;
        }
        if count != 0 && !v.cache.is_null() {
            (*v.cache).invalidate(kind)
        }
        (v.device.write)(v.device.context, lba, count, data.data, data.len)
    }
}
pub(super) unsafe fn flush(v: Io) -> bool {
    unsafe { (v.device.flush)(v.device.context) }
}
fn span(buffer: Buffer, start: usize, len: usize) -> Buffer {
    assert!(start <= buffer.len && len <= buffer.len - start);
    Buffer {
        data: buffer.data.wrapping_add(start),
        len,
    }
}
unsafe fn advance(progress: *mut usize, n: usize) {
    if !progress.is_null() {
        unsafe { *progress += n }
    }
}
pub(super) unsafe fn lcn(
    v: Io,
    lcn: u64,
    byte_offset: u64,
    buffer: Buffer,
    kind: Option<Mutation>,
    progress: *mut usize,
) -> bool {
    unsafe {
        let g = v.geometry;
        if g.cluster_bytes == 0 || !g.cluster_bytes.is_multiple_of(512) {
            return false;
        }
        let total = g.total_clusters();
        if lcn >= total {
            return false;
        }
        let Some(window) = (total - lcn).checked_mul(u64::from(g.cluster_bytes)) else {
            return false;
        };
        if byte_offset > window || buffer.len as u64 > window - byte_offset {
            return false;
        }
        let Some(bytes) = g.total_sectors.checked_mul(512) else {
            return false;
        };
        let Some(base) = lcn.checked_mul(u64::from(g.cluster_bytes)) else {
            return false;
        };
        let Some(mut offset) = base.checked_add(byte_offset) else {
            return false;
        };
        if offset > bytes || buffer.len as u64 > bytes - offset {
            return false;
        }
        let mut pos = 0;
        let mut remaining = buffer.len;
        while remaining > 0 {
            let Some(lba) = u64::from(g.partition_lba).checked_add(offset / 512) else {
                return false;
            };
            let inside = (offset % 512) as usize;
            if inside == 0 && remaining >= 512 {
                let whole = remaining / 512;
                let limit = if v.device.max_transfer_sectors == 0 {
                    256
                } else {
                    v.device.max_transfer_sectors
                };
                let count = whole.min(limit.min(256) as usize) as u32;
                let piece = span(buffer, pos, count as usize * 512);
                let ok = if let Some(kind) = kind {
                    write(v, lba, count, piece, kind)
                } else {
                    read(v, lba, count, piece)
                };
                if !ok {
                    return false;
                }
                advance(progress, piece.len);
                pos += piece.len;
                offset += piece.len as u64;
                remaining -= piece.len;
                continue;
            }
            let sector = Buffer {
                data: ptr::addr_of_mut!((*v.scratch).sector).cast::<u8>(),
                len: 512,
            };
            if !read(v, lba, 1, sector) {
                return false;
            }
            let n = (512 - inside).min(remaining);
            if let Some(kind) = kind {
                ptr::copy_nonoverlapping(buffer.data.add(pos), sector.data.add(inside), n);
                if !write(v, lba, 1, sector, kind) {
                    return false;
                }
            } else {
                ptr::copy_nonoverlapping(sector.data.add(inside), buffer.data.add(pos), n)
            }
            advance(progress, n);
            pos += n;
            offset += n as u64;
            remaining -= n;
        }
        true
    }
}
pub(super) unsafe fn runs_valid(v: Io, runs: Runs) -> bool {
    unsafe {
        let g = v.geometry;
        if g.cluster_bytes == 0
            || !g.cluster_bytes.is_multiple_of(512)
            || g.total_sectors.checked_mul(512).is_none()
        {
            return false;
        }
        let mut logical = 0u64;
        for i in 0..runs.len {
            let r = runs.get(i);
            if r.length_clusters == 0 {
                return false;
            }
            let Some(next) = logical.checked_add(r.length_clusters) else {
                return false;
            };
            logical = next;
            if !g.mapped_run_valid(r) {
                return false;
            }
        }
        true
    }
}
pub(super) unsafe fn runs(
    v: Io,
    runs: Runs,
    byte_offset: u64,
    buffer: Buffer,
    kind: Option<Mutation>,
    progress: *mut usize,
) -> bool {
    unsafe {
        let cluster = u64::from(v.geometry.cluster_bytes);
        if cluster == 0 || !runs_valid(v, runs) {
            return false;
        }
        let mut start = byte_offset;
        let mut remaining = buffer.len as u64;
        let mut pos = 0;
        let mut run_start = 0u64;
        for i in 0..runs.len {
            let r = runs.get(i);
            let Some(bytes) = r.length_clusters.checked_mul(cluster) else {
                return false;
            };
            let Some(end) = run_start.checked_add(bytes) else {
                return false;
            };
            if remaining == 0 {
                break;
            }
            if start < end {
                if start < run_start {
                    return false;
                }
                let inside = start - run_start;
                let take = (end - start).min(remaining);
                let piece = span(buffer, pos, take as usize);
                if r.has_lcn {
                    if !lcn(v, r.lcn, inside, piece, kind, progress) {
                        return false;
                    }
                } else if kind.is_some() {
                    return false;
                } else {
                    ptr::write_bytes(piece.data, 0, piece.len);
                    if v.work_counters {
                        (*v.scratch).work.read_zero_bytes += piece.len as u64
                    }
                }
                pos += piece.len;
                start += take;
                remaining -= take;
            }
            run_start = end;
        }
        remaining == 0
    }
}
