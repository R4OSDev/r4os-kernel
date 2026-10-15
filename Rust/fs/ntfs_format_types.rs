//! Private neutral NTFS codec records. Pure borrowed byte views only.
use core::ptr;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct View {
    pub data: *const u8,
    pub len: usize,
}
impl View {
    pub fn of(s: &[u8]) -> Self {
        Self {
            data: s.as_ptr(),
            len: s.len(),
        }
    }
    pub(super) unsafe fn get<'a>(self) -> &'a [u8] {
        if self.len == 0 {
            &[]
        } else {
            unsafe { core::slice::from_raw_parts(self.data, self.len) }
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Buffer {
    pub data: *mut u8,
    pub len: usize,
}
impl Buffer {
    pub(super) unsafe fn get<'a>(self) -> &'a mut [u8] {
        if self.len == 0 {
            &mut []
        } else {
            unsafe { core::slice::from_raw_parts_mut(self.data, self.len) }
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct FileReference {
    pub record: u64,
    pub sequence: u16,
}
impl FileReference {
    pub fn parse(raw: u64) -> Self {
        Self {
            record: raw & 0x0000_ffff_ffff_ffff,
            sequence: (raw >> 48) as u16,
        }
    }
    pub fn pack(self) -> u64 {
        (self.record & 0x0000_ffff_ffff_ffff) | (u64::from(self.sequence) << 48)
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BootSector {
    pub bytes_per_sector: u16,
    pub sectors_per_cluster_raw: u8,
    pub cluster_bytes: u32,
    pub total_sectors: u64,
    pub mft_lcn: u64,
    pub mftmirr_lcn: u64,
    pub file_record_bytes: u32,
    pub index_block_bytes: u32,
    pub serial_number: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct FileRecordHeader {
    pub magic: u32,
    pub lsn: u64,
    pub sequence: u16,
    pub link_count: u16,
    pub attrs_offset: u16,
    pub flags: u16,
    pub bytes_in_use: u32,
    pub bytes_allocated: u32,
    pub base_record: FileReference,
    pub next_attr_instance: u16,
    pub record_number: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Attribute {
    pub attr_type: u32,
    pub length: u32,
    pub non_resident: bool,
    pub name: View,
    pub flags: u16,
    pub instance: u16,
    pub value: View,
    pub lowest_vcn: u64,
    pub highest_vcn: u64,
    pub mapping_pairs: View,
    pub compression_unit: u8,
    pub allocated_size: u64,
    pub data_size: u64,
    pub initialized_size: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Run {
    pub length_clusters: u64,
    pub has_lcn: bool,
    pub lcn: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct StandardInformation {
    pub creation_time: u64,
    pub data_change_time: u64,
    pub mft_change_time: u64,
    pub access_time: u64,
    pub file_attributes: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct FileName {
    pub parent: FileReference,
    pub creation_time: u64,
    pub data_change_time: u64,
    pub mft_change_time: u64,
    pub access_time: u64,
    pub allocated_size: u64,
    pub data_size: u64,
    pub flags: u32,
    pub name_length: u8,
    pub namespace: u8,
    pub name: View,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct AttributeListEntry {
    pub attr_type: u32,
    pub entry_length: u16,
    pub lowest_vcn: u64,
    pub mft_reference: FileReference,
    pub instance: u16,
    pub name: View,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct IndexHeader {
    pub entries_offset: u32,
    pub index_length: u32,
    pub allocated_size: u32,
    pub flags: u8,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct IndexRoot {
    pub indexed_attr_type: u32,
    pub collation_rule: u32,
    pub index_block_bytes: u32,
    pub header: IndexHeader,
    pub entries: View,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct IndexBlock {
    pub lsn: u64,
    pub vcn: u64,
    pub entries: View,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct IndexEntry {
    pub file_reference: u64,
    pub entry_length: u16,
    pub key_length: u16,
    pub flags: u16,
    pub key: View,
    pub has_sub_node_vcn: bool,
    pub sub_node_vcn: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct VolumeInformation {
    pub major: u8,
    pub minor: u8,
    pub flags: u16,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct IteratorState {
    pub offset: usize,
    pub current_lcn: i64,
    pub failed: bool,
    pub done: bool,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Args {
    pub bytes: View,
    pub second: View,
    pub third: View,
    pub buffer: Buffer,
    pub input: *const core::ffi::c_void,
    pub output: *mut core::ffi::c_void,
    pub state: *mut IteratorState,
    pub value: u64,
    pub signed: i64,
    pub flag: bool,
}
pub fn le16(s: &[u8], o: usize) -> u16 {
    u16::from_le_bytes(s[o..o + 2].try_into().expect("NTFSu16"))
}
pub fn le32(s: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(s[o..o + 4].try_into().expect("NTFSu32"))
}
pub fn le64(s: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(s[o..o + 8].try_into().expect("NTFSu64"))
}
pub(super) unsafe fn publish<T>(out: *mut core::ffi::c_void, value: Option<T>) -> bool {
    if let Some(value) = value {
        unsafe { ptr::write(out.cast::<T>(), value) };
        true
    } else {
        false
    }
}

const _: () = {
    assert!(core::mem::size_of::<View>() == 16);
    assert!(core::mem::align_of::<View>() == 8);
    assert!(core::mem::offset_of!(View, data) == 0);
    assert!(core::mem::offset_of!(View, len) == 8);
    assert!(core::mem::size_of::<Buffer>() == 16);
    assert!(core::mem::align_of::<Buffer>() == 8);
    assert!(core::mem::offset_of!(Buffer, data) == 0);
    assert!(core::mem::offset_of!(Buffer, len) == 8);
    assert!(core::mem::size_of::<FileReference>() == 16);
    assert!(core::mem::align_of::<FileReference>() == 8);
    assert!(core::mem::offset_of!(FileReference, record) == 0);
    assert!(core::mem::offset_of!(FileReference, sequence) == 8);
    assert!(core::mem::size_of::<BootSector>() == 48);
    assert!(core::mem::align_of::<BootSector>() == 8);
    assert!(core::mem::offset_of!(BootSector, bytes_per_sector) == 0);
    assert!(core::mem::offset_of!(BootSector, sectors_per_cluster_raw) == 2);
    assert!(core::mem::offset_of!(BootSector, cluster_bytes) == 4);
    assert!(core::mem::offset_of!(BootSector, total_sectors) == 8);
    assert!(core::mem::offset_of!(BootSector, mft_lcn) == 16);
    assert!(core::mem::offset_of!(BootSector, mftmirr_lcn) == 24);
    assert!(core::mem::offset_of!(BootSector, file_record_bytes) == 32);
    assert!(core::mem::offset_of!(BootSector, index_block_bytes) == 36);
    assert!(core::mem::offset_of!(BootSector, serial_number) == 40);
    assert!(core::mem::size_of::<FileRecordHeader>() == 56);
    assert!(core::mem::align_of::<FileRecordHeader>() == 8);
    assert!(core::mem::offset_of!(FileRecordHeader, magic) == 0);
    assert!(core::mem::offset_of!(FileRecordHeader, lsn) == 8);
    assert!(core::mem::offset_of!(FileRecordHeader, sequence) == 16);
    assert!(core::mem::offset_of!(FileRecordHeader, link_count) == 18);
    assert!(core::mem::offset_of!(FileRecordHeader, attrs_offset) == 20);
    assert!(core::mem::offset_of!(FileRecordHeader, flags) == 22);
    assert!(core::mem::offset_of!(FileRecordHeader, bytes_in_use) == 24);
    assert!(core::mem::offset_of!(FileRecordHeader, bytes_allocated) == 28);
    assert!(core::mem::offset_of!(FileRecordHeader, base_record) == 32);
    assert!(core::mem::offset_of!(FileRecordHeader, next_attr_instance) == 48);
    assert!(core::mem::offset_of!(FileRecordHeader, record_number) == 52);
    assert!(core::mem::size_of::<Attribute>() == 120);
    assert!(core::mem::align_of::<Attribute>() == 8);
    assert!(core::mem::offset_of!(Attribute, attr_type) == 0);
    assert!(core::mem::offset_of!(Attribute, length) == 4);
    assert!(core::mem::offset_of!(Attribute, non_resident) == 8);
    assert!(core::mem::offset_of!(Attribute, name) == 16);
    assert!(core::mem::offset_of!(Attribute, flags) == 32);
    assert!(core::mem::offset_of!(Attribute, instance) == 34);
    assert!(core::mem::offset_of!(Attribute, value) == 40);
    assert!(core::mem::offset_of!(Attribute, lowest_vcn) == 56);
    assert!(core::mem::offset_of!(Attribute, highest_vcn) == 64);
    assert!(core::mem::offset_of!(Attribute, mapping_pairs) == 72);
    assert!(core::mem::offset_of!(Attribute, compression_unit) == 88);
    assert!(core::mem::offset_of!(Attribute, allocated_size) == 96);
    assert!(core::mem::offset_of!(Attribute, data_size) == 104);
    assert!(core::mem::offset_of!(Attribute, initialized_size) == 112);
    assert!(core::mem::size_of::<Run>() == 24);
    assert!(core::mem::align_of::<Run>() == 8);
    assert!(core::mem::offset_of!(Run, length_clusters) == 0);
    assert!(core::mem::offset_of!(Run, has_lcn) == 8);
    assert!(core::mem::offset_of!(Run, lcn) == 16);
    assert!(core::mem::size_of::<StandardInformation>() == 40);
    assert!(core::mem::align_of::<StandardInformation>() == 8);
    assert!(core::mem::offset_of!(StandardInformation, creation_time) == 0);
    assert!(core::mem::offset_of!(StandardInformation, data_change_time) == 8);
    assert!(core::mem::offset_of!(StandardInformation, mft_change_time) == 16);
    assert!(core::mem::offset_of!(StandardInformation, access_time) == 24);
    assert!(core::mem::offset_of!(StandardInformation, file_attributes) == 32);
    assert!(core::mem::size_of::<FileName>() == 88);
    assert!(core::mem::align_of::<FileName>() == 8);
    assert!(core::mem::offset_of!(FileName, parent) == 0);
    assert!(core::mem::offset_of!(FileName, creation_time) == 16);
    assert!(core::mem::offset_of!(FileName, data_change_time) == 24);
    assert!(core::mem::offset_of!(FileName, mft_change_time) == 32);
    assert!(core::mem::offset_of!(FileName, access_time) == 40);
    assert!(core::mem::offset_of!(FileName, allocated_size) == 48);
    assert!(core::mem::offset_of!(FileName, data_size) == 56);
    assert!(core::mem::offset_of!(FileName, flags) == 64);
    assert!(core::mem::offset_of!(FileName, name_length) == 68);
    assert!(core::mem::offset_of!(FileName, namespace) == 69);
    assert!(core::mem::offset_of!(FileName, name) == 72);
    assert!(core::mem::size_of::<AttributeListEntry>() == 56);
    assert!(core::mem::align_of::<AttributeListEntry>() == 8);
    assert!(core::mem::offset_of!(AttributeListEntry, attr_type) == 0);
    assert!(core::mem::offset_of!(AttributeListEntry, entry_length) == 4);
    assert!(core::mem::offset_of!(AttributeListEntry, lowest_vcn) == 8);
    assert!(core::mem::offset_of!(AttributeListEntry, mft_reference) == 16);
    assert!(core::mem::offset_of!(AttributeListEntry, instance) == 32);
    assert!(core::mem::offset_of!(AttributeListEntry, name) == 40);
    assert!(core::mem::size_of::<IndexRoot>() == 48);
    assert!(core::mem::align_of::<IndexRoot>() == 8);
    assert!(core::mem::offset_of!(IndexRoot, indexed_attr_type) == 0);
    assert!(core::mem::offset_of!(IndexRoot, collation_rule) == 4);
    assert!(core::mem::offset_of!(IndexRoot, index_block_bytes) == 8);
    assert!(core::mem::offset_of!(IndexRoot, header) == 12);
    assert!(core::mem::offset_of!(IndexRoot, entries) == 32);
    assert!(core::mem::size_of::<IndexHeader>() == 16);
    assert!(core::mem::align_of::<IndexHeader>() == 4);
    assert!(core::mem::offset_of!(IndexHeader, entries_offset) == 0);
    assert!(core::mem::offset_of!(IndexHeader, index_length) == 4);
    assert!(core::mem::offset_of!(IndexHeader, allocated_size) == 8);
    assert!(core::mem::offset_of!(IndexHeader, flags) == 12);
    assert!(core::mem::size_of::<IndexBlock>() == 32);
    assert!(core::mem::align_of::<IndexBlock>() == 8);
    assert!(core::mem::offset_of!(IndexBlock, lsn) == 0);
    assert!(core::mem::offset_of!(IndexBlock, vcn) == 8);
    assert!(core::mem::offset_of!(IndexBlock, entries) == 16);
    assert!(core::mem::size_of::<IndexEntry>() == 48);
    assert!(core::mem::align_of::<IndexEntry>() == 8);
    assert!(core::mem::offset_of!(IndexEntry, file_reference) == 0);
    assert!(core::mem::offset_of!(IndexEntry, entry_length) == 8);
    assert!(core::mem::offset_of!(IndexEntry, key_length) == 10);
    assert!(core::mem::offset_of!(IndexEntry, flags) == 12);
    assert!(core::mem::offset_of!(IndexEntry, key) == 16);
    assert!(core::mem::offset_of!(IndexEntry, has_sub_node_vcn) == 32);
    assert!(core::mem::offset_of!(IndexEntry, sub_node_vcn) == 40);
    assert!(core::mem::size_of::<VolumeInformation>() == 4);
    assert!(core::mem::align_of::<VolumeInformation>() == 2);
    assert!(core::mem::offset_of!(VolumeInformation, major) == 0);
    assert!(core::mem::offset_of!(VolumeInformation, minor) == 1);
    assert!(core::mem::offset_of!(VolumeInformation, flags) == 2);
    assert!(core::mem::size_of::<IteratorState>() == 24);
    assert!(core::mem::align_of::<IteratorState>() == 8);
    assert!(core::mem::offset_of!(IteratorState, offset) == 0);
    assert!(core::mem::offset_of!(IteratorState, current_lcn) == 8);
    assert!(core::mem::offset_of!(IteratorState, failed) == 16);
    assert!(core::mem::offset_of!(IteratorState, done) == 17);
    assert!(core::mem::size_of::<Args>() == 112);
    assert!(core::mem::align_of::<Args>() == 8);
    assert!(core::mem::offset_of!(Args, bytes) == 0);
    assert!(core::mem::offset_of!(Args, second) == 16);
    assert!(core::mem::offset_of!(Args, third) == 32);
    assert!(core::mem::offset_of!(Args, buffer) == 48);
    assert!(core::mem::offset_of!(Args, input) == 64);
    assert!(core::mem::offset_of!(Args, output) == 72);
    assert!(core::mem::offset_of!(Args, state) == 80);
    assert!(core::mem::offset_of!(Args, value) == 88);
    assert!(core::mem::offset_of!(Args, signed) == 96);
    assert!(core::mem::offset_of!(Args, flag) == 104);
    assert!(core::mem::size_of::<super::ntfs_format::Result>() == 16);
    assert!(core::mem::align_of::<super::ntfs_format::Result>() == 8);
    assert!(core::mem::offset_of!(super::ntfs_format::Result, value) == 0);
    assert!(core::mem::offset_of!(super::ntfs_format::Result, code) == 8);
    assert!(core::mem::offset_of!(super::ntfs_format::Result, present) == 12);
};
