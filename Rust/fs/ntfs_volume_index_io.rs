//! Original direct/cached INDX I/O and private raw entry geometry.
use super::{
    ntfs_format_records as fmt, ntfs_format_types::*, ntfs_volume_edit as edit,
    ntfs_volume_io as io, ntfs_volume_records as r, ntfs_volume_types::*,
};
use core::ptr;
pub const I30: [u8; 8] = [b'$', 0, b'I', 0, b'3', 0, b'0', 0];
pub(super) unsafe fn block(v: Volume) -> Buffer {
    unsafe {
        assert!(v.index_block_bytes as usize <= 4096 + ENTRY_MAX);
        Buffer {
            data: ptr::addr_of_mut!((*v.scratch()).block).cast(),
            len: v.index_block_bytes as usize,
        }
    }
}
pub(super) unsafe fn load_into(v: Volume, runs: io::Runs, vcn: u64, out: Buffer) -> bool {
    unsafe {
        let b = r::part(out, 0, v.index_block_bytes as usize);
        let Some(offset) = vcn.checked_mul(u64::from(v.geometry().cluster_bytes)) else {
            return false;
        };
        if !io::runs(v.io, runs, offset, b, None, ptr::null_mut()) {
            return false;
        }
        fmt::fixup(r::mutable(b), None) == 0
    }
}
pub(super) unsafe fn load(v: Volume, runs: io::Runs, vcn: u64) -> bool {
    unsafe {
        v.geometry().cluster_bytes <= v.index_block_bytes && load_into(v, runs, vcn, block(v))
    }
}
pub(super) unsafe fn cached(v: Volume, dir: u64, runs: io::Runs, vcn: u64) -> bool {
    unsafe {
        let b = block(v);
        if !v.cache().is_null() && (*v.cache()).lookup_index(dir, vcn, r::mutable(b)) {
            return true;
        }
        if !load(v, runs, vcn) {
            return false;
        }
        if !v.cache().is_null() {
            (*v.cache()).store_index(dir, vcn, r::view(b))
        }
        true
    }
}
pub(super) unsafe fn store_from(
    v: Volume,
    dir: u64,
    runs: io::Runs,
    vcn: u64,
    buffer: Buffer,
) -> bool {
    unsafe {
        let b = r::part(buffer, 0, v.index_block_bytes as usize);
        let usn = le16(r::view(b), usize::from(le16(r::view(b), 4)));
        if fmt::fixup(r::mutable(b), Some(usn)) != 0 {
            return false;
        }
        let Some(offset) = vcn.checked_mul(u64::from(v.geometry().cluster_bytes)) else {
            return false;
        };
        if !io::runs(
            v.io,
            runs,
            offset,
            b,
            Some(Mutation::Directory(dir)),
            ptr::null_mut(),
        ) {
            return false;
        }
        let _ = fmt::fixup(r::mutable(b), None);
        true
    }
}
pub(super) unsafe fn store(v: Volume, dir: u64, runs: io::Runs, vcn: u64) -> bool {
    unsafe { store_from(v, dir, runs, vcn, block(v)) }
}
pub fn entry_len(bytes: &[u8], pos: usize) -> usize {
    le16(bytes, pos + 8) as usize
}
pub fn entry_end(bytes: &[u8], pos: usize) -> bool {
    le16(bytes, pos + 12) & 2 != 0
}
pub fn entry_child(bytes: &[u8], pos: usize) -> Option<u64> {
    if le16(bytes, pos + 12) & 1 == 0 {
        None
    } else {
        Some(le64(bytes, pos + entry_len(bytes, pos) - 8))
    }
}
pub fn set_child(bytes: &mut [u8], pos: usize, vcn: u64) {
    let n = entry_len(bytes, pos);
    r::put64(bytes, pos + n - 8, vcn)
}
pub fn entry_name(bytes: &[u8], pos: usize) -> &[u8] {
    let n = bytes[pos + 0x50] as usize;
    &bytes[pos + 0x52..pos + 0x52 + n * 2]
}
pub fn has_children(bytes: &[u8]) -> bool {
    bytes[0x24] & 1 != 0
}
pub fn end_child(bytes: &[u8]) -> Option<u64> {
    let mut pos = 24 + le32(bytes, 24) as usize;
    let end = 24 + le32(bytes, 28) as usize;
    while pos < end {
        if entry_end(bytes, pos) {
            return entry_child(bytes, pos);
        }
        pos += entry_len(bytes, pos)
    }
    None
}
pub fn has_entries(bytes: &[u8]) -> bool {
    !entry_end(bytes, 24 + le32(bytes, 24) as usize)
}
pub fn header(v: Volume, b: &mut [u8], vcn: u64, used: usize, children: bool) {
    r::put32(b, 0, 0x58444e49);
    r::put16(b, 4, 0x28);
    r::put16(
        b,
        6,
        u16::try_from(v.index_block_bytes / 512 + 1).expect("NTFS USA count"),
    );
    r::put64(b, 8, 0);
    r::put64(b, 16, vcn);
    r::put32(b, 24, 0x28);
    r::put32(b, 28, u32::try_from(used - 24).expect("NTFS index used"));
    r::put32(b, 32, v.index_block_bytes - 24);
    b[36] = u8::from(children);
    b[37] = 0;
    r::put16(b, 38, 0)
}
#[derive(Clone, Copy)]
pub struct Root {
    pub span: edit::Span,
    pub value_offset: usize,
    pub header_at: usize,
}
impl Root {
    pub fn locate(record: &[u8], h: FileRecordHeader) -> Option<Self> {
        let span = edit::find(record, h, 0x90, &I30)?;
        let offset = le16(record, span.offset + 0x14) as usize;
        Some(Self {
            span,
            value_offset: offset,
            header_at: span.offset + offset + 16,
        })
    }
}
