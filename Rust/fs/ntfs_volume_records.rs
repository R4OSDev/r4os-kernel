//! Original NTFS MFT bootstrap, records and attribute collection.
use super::{
    ntfs_format_records as fmt, ntfs_format_types::*, ntfs_volume_attributes as attrs,
    ntfs_volume_io as io, ntfs_volume_types::*,
};
use core::ptr;
pub(super) unsafe fn view<'a>(s: Buffer) -> &'a [u8] {
    if s.len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(s.data, s.len) }
    }
}
pub(super) unsafe fn mutable<'a>(s: Buffer) -> &'a mut [u8] {
    if s.len == 0 {
        &mut []
    } else {
        unsafe { core::slice::from_raw_parts_mut(s.data, s.len) }
    }
}
pub fn part(s: Buffer, start: usize, len: usize) -> Buffer {
    assert!(start <= s.len && len <= s.len - start);
    Buffer {
        data: s.data.wrapping_add(start),
        len,
    }
}
pub fn put16(s: &mut [u8], o: usize, v: u16) {
    s[o..o + 2].copy_from_slice(&v.to_le_bytes())
}
pub fn put32(s: &mut [u8], o: usize, v: u32) {
    s[o..o + 4].copy_from_slice(&v.to_le_bytes())
}
pub fn put64(s: &mut [u8], o: usize, v: u64) {
    s[o..o + 8].copy_from_slice(&v.to_le_bytes())
}
pub(super) unsafe fn attr_runs(a: *const AttrScratch) -> io::Runs {
    unsafe {
        assert!((*a).count <= 256);
        io::Runs {
            data: ptr::addr_of!((*a).runs).cast::<Run>(),
            len: (*a).count,
        }
    }
}
pub(super) unsafe fn mft_io(
    v: Volume,
    offset: u64,
    buffer: Buffer,
    kind: Option<Mutation>,
) -> bool {
    unsafe {
        if *v.mft_count > v.mft_capacity {
            return false;
        }
        io::runs(
            v.io,
            io::Runs {
                data: v.mft_runs,
                len: *v.mft_count,
            },
            offset,
            buffer,
            kind,
            ptr::null_mut(),
        )
    }
}
#[inline(never)]
pub(super) unsafe fn load_raw(v: Volume, number: u64, buffer: Buffer) -> bool {
    unsafe {
        if v.record_bytes == 0 || v.record_bytes as usize > buffer.len {
            return false;
        }
        let out = part(buffer, 0, v.record_bytes as usize);
        let s = v.scratch();
        if number == 0 && (*s).has_mirror_bootstrap_lcn {
            return io::lcn(
                v.io,
                (*s).mirror_bootstrap_lcn,
                0,
                out,
                None,
                ptr::null_mut(),
            );
        }
        let Some(offset) = number.checked_mul(u64::from(v.record_bytes)) else {
            return false;
        };
        mft_io(v, offset, out, None)
    }
}
#[inline(never)]
pub(super) unsafe fn load(v: Volume, number: u64, buffer: Buffer) -> Option<FileRecordHeader> {
    unsafe {
        if v.record_bytes == 0 || v.record_bytes as usize > buffer.len {
            return None;
        }
        let out = part(buffer, 0, v.record_bytes as usize);
        let cached = !v.cache().is_null() && (*v.cache()).lookup_record(number, mutable(out));
        if !cached && (!load_raw(v, number, out) || fmt::fixup(mutable(out), None) != 0) {
            return None;
        }
        let h = fmt::file_header(view(out))?;
        if u64::from(h.record_number) != number || h.flags & 1 == 0 {
            return None;
        }
        if !cached && !v.cache().is_null() {
            (*v.cache()).store_record(number, view(out))
        }
        Some(h)
    }
}
#[inline(never)]
pub(super) unsafe fn store(v: Volume, number: u64, buffer: Buffer) -> bool {
    unsafe {
        if (*v.scratch()).has_mirror_bootstrap_lcn
            || v.record_bytes == 0
            || v.record_bytes as usize > buffer.len
        {
            return false;
        }
        let out = part(buffer, 0, v.record_bytes as usize);
        let usn = if out.len >= 0x32 {
            le16(view(out), usize::from(le16(view(out), 4)))
        } else {
            0
        };
        if fmt::fixup(mutable(out), Some(usn)) != 0 {
            return false;
        }
        let Some(offset) = number.checked_mul(u64::from(v.record_bytes)) else {
            return false;
        };
        if !mft_io(v, offset, out, Some(Mutation::Record(number))) {
            return false;
        }
        if number < 4 && !sync_mirror(v) {
            return false;
        }
        let _ = fmt::fixup(mutable(out), None);
        if !v.cache().is_null() {
            (*v.cache()).store_record(number, view(out))
        }
        true
    }
}
#[inline(never)]
pub(super) unsafe fn sync_mirror(v: Volume) -> bool {
    unsafe {
        let Some(n) = u64::from(v.record_bytes).checked_mul(4) else {
            return false;
        };
        if n > 4096 {
            return false;
        }
        let buf = Buffer {
            data: ptr::addr_of_mut!((*v.scratch()).mirror_buf).cast(),
            len: n as usize,
        };
        if !mft_io(v, 0, buf, None) {
            return false;
        }
        let a = ptr::addr_of_mut!((*v.scratch()).attr_mirror);
        if collect(v, 1, 0x80, View::default(), a) != Lookup::Found
            || (*a).resident
            || (*a).count == 0
        {
            return false;
        }
        io::runs(
            v.io,
            attr_runs(a),
            0,
            buf,
            Some(Mutation::System),
            ptr::null_mut(),
        )
    }
}
pub(super) unsafe fn find(
    v: Volume,
    record: Buffer,
    h: FileRecordHeader,
    kind: u32,
    name: View,
) -> Option<Attribute> {
    let _ = v;
    unsafe {
        let mut offset = usize::from(h.attrs_offset);
        while let Some(a) = fmt::attribute(view(record), &mut offset) {
            if a.attr_type == kind && a.name.get() == name.get() {
                return Some(a);
            }
        }
        None
    }
}
#[inline(never)]
pub(super) unsafe fn collect(
    v: Volume,
    number: u64,
    kind: u32,
    name: View,
    out: *mut AttrScratch,
) -> Lookup {
    unsafe {
        if !v.cache().is_null()
            && (*v.cache()).lookup_attribute(number, kind, name.get(), &mut *out)
        {
            return Lookup::Found;
        }
        (*out).clear();
        let Some(h) = load(v, number, v.record()) else {
            return Lookup::Io;
        };
        let record = v.record();
        if number > u64::from(u32::MAX)
            || h.flags & 1 == 0
            || u64::from(h.record_number) != number
            || !attrs::stream_valid(view(record), h)
        {
            return Lookup::Io;
        }
        let mut geometry = attrs::AttributeGeometry::default();
        if let Some(list) = find(v, record, h, 0x20, View::default()) {
            if list.non_resident {
                return Lookup::Io;
            }
            let value = list.value;
            let mut offset = 0;
            let mut found_any = false;
            let mut listed_match = false;
            while let Some(e) = fmt::attr_list(value.get(), &mut offset) {
                if e.attr_type != kind || e.name.get() != name.get() {
                    continue;
                }
                listed_match = true;
                let Some(ph) = load(v, e.mft_reference.record, v.part_record()) else {
                    return Lookup::Io;
                };
                if ph.flags & 1 == 0
                    || u64::from(ph.record_number) != e.mft_reference.record
                    || ph.sequence != e.mft_reference.sequence
                {
                    return Lookup::Io;
                }
                if e.mft_reference.record != number
                    && (ph.base_record.record != number || ph.base_record.sequence != h.sequence)
                {
                    return Lookup::Io;
                }
                let part = v.part_record();
                if !attrs::stream_valid(view(part), ph) {
                    return Lookup::Io;
                }
                let mut po = usize::from(ph.attrs_offset);
                let mut matches = 0;
                while let Some(a) = fmt::attribute(view(part), &mut po) {
                    if a.attr_type != kind || a.name.get() != name.get() || a.instance != e.instance
                    {
                        continue;
                    }
                    if (a.non_resident && a.lowest_vcn != e.lowest_vcn)
                        || (!a.non_resident && e.lowest_vcn != 0)
                    {
                        return Lookup::Io;
                    }
                    matches += 1;
                    if matches != 1 || !attrs::capture(v.geometry(), a, &mut *out, &mut geometry) {
                        return Lookup::Io;
                    }
                    found_any = true
                }
                if matches != 1 {
                    return Lookup::Io;
                }
            }
            if offset != value.len || (listed_match && !found_any) {
                return Lookup::Io;
            }
            if found_any {
                if !geometry.complete() {
                    return Lookup::Io;
                }
                if !v.cache().is_null() {
                    (*v.cache()).store_attribute(number, kind, name.get(), &*out)
                }
                return Lookup::Found;
            }
        }
        let mut offset = usize::from(h.attrs_offset);
        let mut found = false;
        while let Some(a) = fmt::attribute(view(record), &mut offset) {
            if a.attr_type != kind || a.name.get() != name.get() {
                continue;
            }
            if !attrs::capture(v.geometry(), a, &mut *out, &mut geometry) {
                return Lookup::Io;
            }
            found = true
        }
        if !found {
            return Lookup::NotFound;
        }
        if !geometry.complete() {
            return Lookup::Io;
        }
        if !v.cache().is_null() {
            (*v.cache()).store_attribute(number, kind, name.get(), &*out)
        }
        Lookup::Found
    }
}
#[inline(never)]
pub(super) unsafe fn bootstrap(v: Volume, source: u64, primary: u64) -> Option<usize> {
    unsafe {
        let record = v.record();
        if !io::lcn(v.io, source, 0, record, None, ptr::null_mut())
            || fmt::fixup(mutable(record), None) != 0
        {
            return None;
        }
        let h = fmt::file_header(view(record))?;
        if h.flags & 1 == 0 || h.record_number != 0 || !attrs::stream_valid(view(record), h) {
            return None;
        }
        let out = ptr::addr_of_mut!((*v.scratch()).attr);
        (*out).clear();
        let mut geometry = attrs::AttributeGeometry::default();
        let mut offset = usize::from(h.attrs_offset);
        let mut found = false;
        while let Some(a) = fmt::attribute(view(record), &mut offset) {
            if a.attr_type != 0x80 || a.name.len != 0 {
                continue;
            }
            if !a.non_resident || !attrs::capture(v.geometry(), a, &mut *out, &mut geometry) {
                return None;
            }
            found = true
        }
        if !found
            || !geometry.complete()
            || (*out).count == 0
            || (*out).count > v.mft_capacity
            || (*out).data_size < u64::from(v.record_bytes)
            || (*out).initialized_size < u64::from(v.record_bytes)
            || !(*out).runs[0].has_lcn
            || (*out).runs[0].lcn != primary
        {
            return None;
        }
        ptr::copy_nonoverlapping(
            ptr::addr_of!((*out).runs).cast::<Run>(),
            v.mft_runs,
            (*out).count,
        );
        Some((*out).count)
    }
}
#[inline(never)]
pub(super) unsafe fn mount(
    device: io::Device,
    partition: u32,
    scratch: *mut Scratch,
    runs: *mut Run,
    capacity: usize,
    work: bool,
) -> Option<MountInfo> {
    unsafe {
        let mut boot_bytes = [0u8; 512];
        if !(device.read)(
            device.context,
            u64::from(partition),
            1,
            boot_bytes.as_mut_ptr(),
            512,
        ) {
            return None;
        }
        let mut boot = BootSector::default();
        if fmt::boot(&boot_bytes, &mut boot) != 0
            || boot.bytes_per_sector != 512
            || boot.file_record_bytes > 4096
            || boot.index_block_bytes as usize > 4096 + ENTRY_MAX
            || boot.total_sectors.checked_mul(512).is_none()
        {
            return None;
        }
        let spc = boot.cluster_bytes / 512;
        if spc == 0 {
            return None;
        }
        let total = boot.total_sectors / u64::from(spc);
        if boot.mft_lcn >= total || boot.mftmirr_lcn >= total {
            return None;
        }
        let mut count = 0;
        let v = Volume {
            io: io::Io {
                device,
                geometry: Geometry {
                    partition_lba: partition,
                    cluster_bytes: boot.cluster_bytes,
                    total_sectors: boot.total_sectors,
                },
                scratch,
                cache: ptr::null_mut(),
                work_counters: work,
            },
            record_bytes: boot.file_record_bytes,
            index_block_bytes: boot.index_block_bytes,
            mft_runs: runs,
            mft_capacity: capacity,
            mft_count: &mut count,
            upcase: View::default(),
            now_filetime: 0,
            security_id_file: 265,
            security_id_dir: 264,
            cache_now_ticks: 0,
        };
        (*scratch).has_mirror_bootstrap_lcn = false;
        let (count, used) = if let Some(count) = bootstrap(v, boot.mft_lcn, boot.mft_lcn) {
            (count, false)
        } else {
            (bootstrap(v, boot.mftmirr_lcn, boot.mft_lcn)?, true)
        };
        if used {
            (*scratch).has_mirror_bootstrap_lcn = true;
            (*scratch).mirror_bootstrap_lcn = boot.mftmirr_lcn
        }
        (*scratch).cluster_search_start = 0;
        (*scratch).record_search_start = 16;
        Some(MountInfo {
            cluster_bytes: boot.cluster_bytes,
            record_bytes: boot.file_record_bytes,
            index_block_bytes: boot.index_block_bytes,
            total_sectors: boot.total_sectors,
            mft_lcn: boot.mft_lcn,
            mftmirr_lcn: boot.mftmirr_lcn,
            mft_run_count: count,
            used_mft_mirror: used,
        })
    }
}
#[inline(never)]
pub(super) unsafe fn set_dirty(v: Volume, dirty: bool) -> bool {
    unsafe {
        if (*v.scratch()).repair_required || (*v.scratch()).has_mirror_bootstrap_lcn {
            return false;
        }
        let Some(h) = load(v, 3, v.write_record()) else {
            return false;
        };
        let record = v.write_record();
        let Some(a) = find(v, record, h, 0x70, View::default()) else {
            return false;
        };
        let offset = a.value.data as usize - record.data as usize;
        if a.value.len < 12 {
            return false;
        }
        let mut flags = le16(view(record), offset + 10);
        let was = flags & 1 != 0;
        if was == dirty {
            return true;
        }
        if dirty {
            flags |= 1
        } else {
            flags &= !1
        }
        put16(mutable(record), offset + 10, flags);
        if !store(v, 3, record) {
            return false;
        }
        io::flush(v.io)
    }
}
#[inline(never)]
pub(super) unsafe fn is_dirty(v: Volume) -> Option<bool> {
    unsafe {
        let h = load(v, 3, v.write_record())?;
        let record = v.write_record();
        let a = find(v, record, h, 0x70, View::default())?;
        let offset = a.value.data as usize - record.data as usize;
        if a.value.len < 12 {
            return None;
        }
        Some(le16(view(record), offset + 10) & 1 != 0)
    }
}
#[inline(never)]
pub(super) unsafe fn ensure_dirty(v: Volume) -> bool {
    unsafe {
        if (*v.scratch()).repair_required || (*v.scratch()).has_mirror_bootstrap_lcn {
            return false;
        }
        match is_dirty(v) {
            Some(true) => io::flush(v.io),
            Some(false) => set_dirty(v, true),
            None => false,
        }
    }
}
