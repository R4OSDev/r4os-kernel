//! Private FAT state and value records. Caller buffers remain raw through
//! waits; public results use fixed projections.
use crate::storage::access_state::Reference;
use core::ptr;
pub const SECTOR: usize = 512;
pub const NAME_MAX: usize = 768;
pub const UNITS: usize = 255;
pub const LFN_MAX: usize = 20;
pub const SLOT_MAX: usize = 21;
pub const DIR_LIMIT: usize = 4096;
pub const RESERVED: u32 = 0x0fff_fff0;
pub const EOC: u32 = 0x0fff_fff8;
pub const EOC_MARK: u32 = 0x0fff_ffff;
pub const UNKNOWN: u32 = u32::MAX;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Volume {
    pub has_ref: bool,
    pub reference: Reference,
    pub device_index: usize,
    pub partition_lba: u32,
    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,
    pub reserved_sectors: u16,
    pub fat_count: u8,
    pub sectors_per_fat: u32,
    pub total_sectors: u32,
    pub root_cluster: u32,
    pub fs_info_sector: u16,
    pub backup_boot_sector: u16,
    pub write_batch: u64,
}
impl Volume {
    pub fn first_data(self) -> u32 {
        self.partition_lba
            + u32::from(self.reserved_sectors)
            + u32::from(self.fat_count) * self.sectors_per_fat
    }
    pub fn cluster_lba(self, cluster: u32) -> u32 {
        self.first_data() + (cluster - 2) * u32::from(self.sectors_per_cluster)
    }
    pub fn fat_lba(self, cluster: u32) -> u32 {
        self.partition_lba
            + u32::from(self.reserved_sectors)
            + (cluster * 4) / u32::from(self.bytes_per_sector)
    }
    pub fn fat_offset(self, cluster: u32) -> usize {
        ((cluster * 4) % u32::from(self.bytes_per_sector)) as usize
    }
    pub fn cluster_bytes(self) -> u32 {
        u32::from(self.bytes_per_sector) * u32::from(self.sectors_per_cluster)
    }
    pub fn total_clusters(self) -> u32 {
        let meta =
            u32::from(self.reserved_sectors) + u32::from(self.fat_count) * self.sectors_per_fat;
        if self.total_sectors <= meta || self.sectors_per_cluster == 0 {
            0
        } else {
            (self.total_sectors - meta) / u32::from(self.sectors_per_cluster)
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Entry {
    pub name: [u8; NAME_MAX],
    pub name_len: usize,
    pub attr: u8,
    pub first_cluster: u32,
    pub has_directory_lba: bool,
    pub directory_lba: u64,
    pub directory_offset: u16,
    pub size: u32,
    pub created_time: u16,
    pub created_date: u16,
    pub access_date: u16,
    pub modified_time: u16,
    pub modified_date: u16,
}
impl Entry {
    pub const EMPTY: Self = Self {
        name: [0; NAME_MAX],
        name_len: 0,
        attr: 0,
        first_cluster: 0,
        has_directory_lba: false,
        directory_lba: 0,
        directory_offset: 0,
        size: 0,
        created_time: 0,
        created_date: 0,
        access_date: 0,
        modified_time: 0,
        modified_date: 0,
    };
    pub fn is_dir(self) -> bool {
        self.attr & 0x10 != 0
    }
    pub fn read_only(self) -> bool {
        self.attr & 1 != 0
    }
    pub fn hidden_or_system(self) -> bool {
        self.attr & 6 != 0
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DirectoryCursor {
    pub cluster: u32,
    pub successor: u32,
    pub steps: u32,
    pub offset: u32,
    pub state: u32,
    pub lfn_len: u32,
    pub lfn_active: u8,
    pub lfn_total: u8,
    pub lfn_expected: u8,
    pub lfn_checksum: u8,
    pub units: [u16; UNITS],
}
impl DirectoryCursor {
    pub const EMPTY: Self = Self {
        cluster: 0,
        successor: 0,
        steps: 0,
        offset: 0,
        state: 0,
        lfn_len: 0,
        lfn_active: 0,
        lfn_total: 0,
        lfn_expected: 0,
        lfn_checksum: 0,
        units: [0; UNITS],
    };
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Bytes {
    pub data: *const u8,
    pub len: usize,
}
impl Bytes {
    pub fn of(s: &[u8]) -> Self {
        Self {
            data: s.as_ptr(),
            len: s.len(),
        }
    }
    pub fn sub(self, start: usize, len: usize) -> Self {
        assert!(start <= self.len && len <= self.len - start);
        Self {
            data: self.data.wrapping_add(start),
            len,
        }
    }
    pub(super) unsafe fn get(self, i: usize) -> u8 {
        assert!(i < self.len);
        unsafe { self.data.add(i).read() }
    }
    pub(super) unsafe fn copy(self, to: *mut u8) {
        if self.len != 0 {
            unsafe {
                ptr::copy_nonoverlapping(self.data, to, self.len);
            }
        }
    }
    // Pure bounded helpers only. Never retain the borrowed view through I/O.
    pub(super) unsafe fn view<'a>(self) -> &'a [u8] {
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
    pub fn of(s: &mut [u8]) -> Self {
        Self {
            data: s.as_mut_ptr(),
            len: s.len(),
        }
    }
    pub fn sub(self, start: usize, len: usize) -> Self {
        assert!(start <= self.len && len <= self.len - start);
        Self {
            data: self.data.wrapping_add(start),
            len,
        }
    }
    pub fn bytes(self) -> Bytes {
        Bytes {
            data: self.data,
            len: self.len,
        }
    }
    pub(super) unsafe fn put(self, i: usize, v: u8) {
        assert!(i < self.len);
        unsafe {
            self.data.add(i).write(v);
        }
    }
}
#[derive(Clone, Copy, Default)]
pub struct Slot {
    pub lba: u32,
    pub offset: usize,
}
#[derive(Clone, Copy)]
pub struct Location {
    pub lba: u32,
    pub offset: usize,
    pub entry: Entry,
    pub lfn: [Slot; LFN_MAX],
    pub lfn_count: usize,
}
impl Location {
    pub const EMPTY: Self = Self {
        lba: 0,
        offset: 0,
        entry: Entry::EMPTY,
        lfn: [Slot { lba: 0, offset: 0 }; LFN_MAX],
        lfn_count: 0,
    };
}
#[derive(Clone, Copy, Default)]
pub struct Allocation {
    pub first: u32,
    pub last: u32,
}
#[derive(Clone, Copy, Default)]
pub struct Run {
    pub first: u32,
    pub count: usize,
}
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Lookup {
    Found,
    NotFound,
    Io,
}
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Append {
    Ok,
    Invalid,
    NotFound,
    OffsetMismatch,
    TooLarge,
    Io,
}
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Rename {
    Ok,
    NotFound,
    NotAtomic,
    Conflict,
    ReadOnly,
    Io,
}
#[repr(i32)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Replace {
    Ok = 0,
    Invalid = -1,
    NotFound = -2,
    Alias = -3,
    Conflict = -4,
    ReadOnly = -5,
    Io = -6,
    NotAtomic = -7,
}
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Delete {
    Deleted,
    NotFound,
    Mismatch,
    Io,
}
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Next {
    Found,
    NotFound,
    Again,
    Io,
}
pub fn le16(s: &[u8]) -> u16 {
    u16::from(s[0]) | (u16::from(s[1]) << 8)
}
pub fn le32(s: &[u8]) -> u32 {
    u32::from(s[0]) | (u32::from(s[1]) << 8) | (u32::from(s[2]) << 16) | (u32::from(s[3]) << 24)
}
pub fn put16(s: &mut [u8], v: u16) {
    s[..2].copy_from_slice(&v.to_le_bytes());
}
pub fn put32(s: &mut [u8], v: u32) {
    s[..4].copy_from_slice(&v.to_le_bytes());
}
pub fn first_cluster(raw: &[u8]) -> u32 {
    (u32::from(le16(&raw[20..22])) << 16) | u32::from(le16(&raw[26..28]))
}
pub fn put_cluster(raw: &mut [u8], cluster: u32) {
    put16(&mut raw[20..22], (cluster >> 16) as u16);
    put16(&mut raw[26..28], cluster as u16);
}
pub fn is_eoc(v: u32) -> bool {
    (EOC..=0x0fff_ffff).contains(&v)
}
const _: () = {
    assert!(core::mem::size_of::<DirectoryCursor>() == 540);
};

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Args {
    pub volume: Volume,
    pub other: Volume,
    pub entry: *const Entry,
    pub peer: *const Entry,
    pub parent: u32,
    pub offset: usize,
    pub index: usize,
    pub name: Bytes,
    pub second: Bytes,
    pub third: Bytes,
    pub data: Bytes,
    pub buffer: Buffer,
    pub cursor: *mut DirectoryCursor,
    pub first: u64,
    pub count: u64,
    pub flag: bool,
    pub set: u8,
    pub clear: u8,
}

const _: () = {
    assert!(core::mem::size_of::<Volume>() == 72);
    assert!(core::mem::align_of::<Volume>() == 8);
    assert!(core::mem::offset_of!(Volume, has_ref) == 0);
    assert!(core::mem::offset_of!(Volume, reference) == 8);
    assert!(core::mem::offset_of!(Volume, device_index) == 24);
    assert!(core::mem::offset_of!(Volume, partition_lba) == 32);
    assert!(core::mem::offset_of!(Volume, bytes_per_sector) == 36);
    assert!(core::mem::offset_of!(Volume, sectors_per_cluster) == 38);
    assert!(core::mem::offset_of!(Volume, reserved_sectors) == 40);
    assert!(core::mem::offset_of!(Volume, fat_count) == 42);
    assert!(core::mem::offset_of!(Volume, sectors_per_fat) == 44);
    assert!(core::mem::offset_of!(Volume, total_sectors) == 48);
    assert!(core::mem::offset_of!(Volume, root_cluster) == 52);
    assert!(core::mem::offset_of!(Volume, fs_info_sector) == 56);
    assert!(core::mem::offset_of!(Volume, backup_boot_sector) == 58);
    assert!(core::mem::offset_of!(Volume, write_batch) == 64);
    assert!(core::mem::size_of::<Entry>() == 824);
    assert!(core::mem::align_of::<Entry>() == 8);
    assert!(core::mem::offset_of!(Entry, name) == 0);
    assert!(core::mem::offset_of!(Entry, name_len) == 768);
    assert!(core::mem::offset_of!(Entry, attr) == 776);
    assert!(core::mem::offset_of!(Entry, first_cluster) == 780);
    assert!(core::mem::offset_of!(Entry, has_directory_lba) == 784);
    assert!(core::mem::offset_of!(Entry, directory_lba) == 792);
    assert!(core::mem::offset_of!(Entry, directory_offset) == 800);
    assert!(core::mem::offset_of!(Entry, size) == 804);
    assert!(core::mem::offset_of!(Entry, created_time) == 808);
    assert!(core::mem::offset_of!(Entry, created_date) == 810);
    assert!(core::mem::offset_of!(Entry, access_date) == 812);
    assert!(core::mem::offset_of!(Entry, modified_time) == 814);
    assert!(core::mem::offset_of!(Entry, modified_date) == 816);
    assert!(core::mem::size_of::<Bytes>() == 16);
    assert!(core::mem::align_of::<Bytes>() == 8);
    assert!(core::mem::offset_of!(Bytes, data) == 0);
    assert!(core::mem::offset_of!(Bytes, len) == 8);
    assert!(core::mem::size_of::<Buffer>() == 16);
    assert!(core::mem::align_of::<Buffer>() == 8);
    assert!(core::mem::offset_of!(Buffer, data) == 0);
    assert!(core::mem::offset_of!(Buffer, len) == 8);
    assert!(core::mem::size_of::<Args>() == 296);
    assert!(core::mem::align_of::<Args>() == 8);
    assert!(core::mem::offset_of!(Args, volume) == 0);
    assert!(core::mem::offset_of!(Args, other) == 72);
    assert!(core::mem::offset_of!(Args, entry) == 144);
    assert!(core::mem::offset_of!(Args, peer) == 152);
    assert!(core::mem::offset_of!(Args, parent) == 160);
    assert!(core::mem::offset_of!(Args, offset) == 168);
    assert!(core::mem::offset_of!(Args, index) == 176);
    assert!(core::mem::offset_of!(Args, name) == 184);
    assert!(core::mem::offset_of!(Args, second) == 200);
    assert!(core::mem::offset_of!(Args, third) == 216);
    assert!(core::mem::offset_of!(Args, data) == 232);
    assert!(core::mem::offset_of!(Args, buffer) == 248);
    assert!(core::mem::offset_of!(Args, cursor) == 264);
    assert!(core::mem::offset_of!(Args, first) == 272);
    assert!(core::mem::offset_of!(Args, count) == 280);
    assert!(core::mem::offset_of!(Args, flag) == 288);
    assert!(core::mem::offset_of!(Args, set) == 289);
    assert!(core::mem::offset_of!(Args, clear) == 290);
};
