//! Original pure attribute extent geometry/stream validation.
use super::{
    ntfs_format_runs,
    ntfs_format_types::{Attribute, FileRecordHeader, IteratorState, le16, le32},
    ntfs_volume_types::*,
};
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Storage {
    #[default]
    None,
    Resident,
    NonResident,
}
#[derive(Clone, Copy, Default)]
pub struct AttributeGeometry {
    pub storage: Storage,
    pub first_extent_count: u8,
    pub next_vcn: u64,
}
impl AttributeGeometry {
    pub fn complete(self) -> bool {
        self.storage != Storage::None && self.first_extent_count == 1
    }
}
pub(super) unsafe fn capture(
    v: Geometry,
    a: Attribute,
    out: &mut AttrScratch,
    g: &mut AttributeGeometry,
) -> bool {
    unsafe {
        if !a.non_resident {
            if g.storage != Storage::None {
                return false;
            }
            let value = a.value.get();
            if value.len() > out.resident_copy.len() {
                return false;
            }
            g.storage = Storage::Resident;
            g.first_extent_count = 1;
            out.resident = true;
            out.resident_copy[..value.len()].copy_from_slice(value);
            out.resident_len = value.len();
            out.data_size = value.len() as u64;
            out.initialized_size = value.len() as u64;
            out.alloc_size = (value.len() as u64 + 7) & !7u64;
            out.flags = a.flags;
            return true;
        }
        if g.storage == Storage::Resident {
            return false;
        }
        let first = g.storage == Storage::None;
        if first {
            if a.lowest_vcn != 0 {
                return false;
            }
            g.storage = Storage::NonResident;
            g.first_extent_count = 1
        } else if a.lowest_vcn == 0 || a.lowest_vcn != g.next_vcn {
            return false;
        }
        if a.highest_vcn < a.lowest_vcn {
            return false;
        }
        let Some(count) = (a.highest_vcn - a.lowest_vcn).checked_add(1) else {
            return false;
        };
        if first {
            if a.initialized_size > a.data_size {
                return false;
            }
            out.data_size = a.data_size;
            out.initialized_size = a.initialized_size;
            out.alloc_size = a.allocated_size;
            out.flags = a.flags;
            out.compression_unit = a.compression_unit
        }
        let mut run_clusters = 0u64;
        let mut it = IteratorState::default();
        let mapping = a.mapping_pairs.get();
        while let Some(run) = ntfs_format_runs::next(mapping, &mut it) {
            let Some(n) = run_clusters.checked_add(run.length_clusters) else {
                return false;
            };
            run_clusters = n;
            if !v.mapped_run_valid(run) || out.count >= out.runs.len() {
                return false;
            }
            out.runs[out.count] = run;
            out.count += 1
        }
        if it.failed
            || it.offset >= mapping.len()
            || mapping[it.offset] != 0
            || run_clusters != count
        {
            return false;
        }
        let Some(next) = a.highest_vcn.checked_add(1) else {
            return false;
        };
        g.next_vcn = next;
        true
    }
}
pub fn stream_valid(record: &[u8], header: FileRecordHeader) -> bool {
    let limit = header.bytes_in_use as usize;
    let mut offset = usize::from(header.attrs_offset);
    while offset + 4 <= limit {
        if le32(record, offset) == u32::MAX {
            return true;
        }
        if offset + 8 > limit {
            return false;
        }
        let length = le32(record, offset + 4) as usize;
        if length < 0x18 || !length.is_multiple_of(8) || offset + length > limit {
            return false;
        }
        let nonresident = record[offset + 8] != 0;
        let units = record[offset + 9] as usize;
        let name = le16(record, offset + 10) as usize;
        if units > 0 && (name > length || units * 2 > length - name) {
            return false;
        }
        if nonresident {
            if length < 0x40 {
                return false;
            }
            let mapping = le16(record, offset + 0x20) as usize;
            if mapping < 0x40 || mapping > length {
                return false;
            }
        } else {
            let n = le32(record, offset + 0x10) as usize;
            let value = le16(record, offset + 0x14) as usize;
            if value < 0x18 || value > length || n > length - value {
                return false;
            }
        }
        offset += length;
    }
    false
}
