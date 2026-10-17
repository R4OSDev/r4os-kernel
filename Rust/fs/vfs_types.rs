//! Private neutral C records. Public Zig values are projected field by field;
//! no compiler-specific union/optional layout crosses the native boundary.
use super::copy_transfer::Progress;
use crate::storage::access_state::Reference;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Fat {
    pub device: usize,
    pub partition: u32,
    pub sector_bytes: u16,
    pub cluster_sectors: u8,
    pub reserved: u16,
    pub fats: u8,
    pub fat_sectors: u32,
    pub sectors: u32,
    pub root: u32,
    pub fsinfo: u16,
    pub backup: u16,
    pub write_batch: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Ntfs {
    pub slot: u8,
    pub cluster_bytes: u32,
    pub sectors: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Location {
    pub device: usize,
    pub first: u64,
    pub count: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Volume {
    pub kind: u8,
    pub has_ref: bool,
    pub reference: Reference,
    pub fat: Fat,
    pub ntfs: Ntfs,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Entry {
    pub name: [u8; 768],
    pub name_len: usize,
    pub attr: u8,
    pub node: u64,
    pub generation: u16,
    pub directory_lba: u64,
    pub has_lba: bool,
    pub directory_offset: u16,
    pub reparse: bool,
    pub size: u64,
    pub times: [u16; 5],
}
impl Entry {
    pub const EMPTY: Self = Self {
        name: [0; 768],
        name_len: 0,
        attr: 0,
        node: 0,
        generation: 0,
        directory_lba: 0,
        has_lba: false,
        directory_offset: 0,
        reparse: false,
        size: 0,
        times: [0; 5],
    };
    pub fn is_dir(self) -> bool {
        self.attr & 0x10 != 0
    }
    pub fn read_only(self) -> bool {
        self.attr & 1 != 0
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Bytes {
    pub data: *const u8,
    pub len: usize,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Args {
    pub volume: Volume,
    pub other: Volume,
    pub entry: *const Entry,
    pub peer: *const Entry,
    pub parent: u64,
    pub offset: u64,
    pub index: usize,
    pub name: Bytes,
    pub second: Bytes,
    pub third: Bytes,
    pub data: Bytes,
    pub buffer: *mut u8,
    pub buffer_len: usize,
    pub cursor: *mut u64,
    pub progress: *mut Progress,
    pub flag: bool,
    pub read_guard: *mut super::request::Guard,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Result {
    pub value: u64,
    pub status: i32,
    pub present: bool,
}
impl Result {
    pub const fn boolean(value: bool) -> Self {
        Self {
            value: 0,
            status: 0,
            present: value,
        }
    }
    pub const fn code(status: i32) -> Self {
        Self {
            value: 0,
            status,
            present: false,
        }
    }
    pub const fn value(value: u64) -> Self {
        Self {
            value,
            status: 0,
            present: true,
        }
    }
}
// Every operation has one owner. Backend IDs below are raw leaf calls;
// neutral/public decisions and conversions live in the native VFS owner.
pub mod op {
    pub const RESOLVE_PATH: u8 = 0;
    pub const RESOLVE_ENTRY: u8 = 1;
    pub const LOOKUP: u8 = 2;
    pub const RECOVERY_LOOKUP: u8 = 3;
    pub const DIAGNOSTIC: u8 = 4;
    pub const READ: u8 = 5;
    pub const READ_RANGE: u8 = 6;
    pub const WRITE_RANGE: u8 = 7;
    pub const WRITE: u8 = 8;
    pub const APPEND: u8 = 9;
    pub const APPEND_OFFSET: u8 = 10;
    pub const APPEND_DEFERRED: u8 = 11;
    pub const PUBLISH_COPY: u8 = 12;
    pub const COPY: u8 = 13;
    pub const COPY_BUFFERED: u8 = 14;
    pub const MKDIR: u8 = 15;
    pub const DELETE: u8 = 16;
    pub const DELETE_MATCH: u8 = 17;
    pub const RECOVERY_DELETE: u8 = 18;
    pub const NAMES: u8 = 19;
    pub const RMDIR: u8 = 20;
    pub const RENAME: u8 = 21;
    pub const REPLACE: u8 = 22;
    pub const REPLACE_CREATE: u8 = 23;
    pub const FLUSH: u8 = 24;
    pub const READ_DIR: u8 = 25;
    pub const DIR_ENTRY: u8 = 26;
    pub const DIR_NEXT: u8 = 27;
    pub const FREE: u8 = 28;
    pub const LIST_ROOT: u8 = 29;
    pub const SHORT_NAME: u8 = 30;
    pub const SAME_COPY: u8 = 31;
    pub const CHILD_SIZE: u8 = 32;
    pub const LOCATION: u8 = 33;
    pub const COPY_NO_REPLACE: u8 = 34;
}
const _: () = {
    assert!(core::mem::size_of::<Fat>() == 48);
    assert!(core::mem::size_of::<Ntfs>() == 16);
    assert!(core::mem::size_of::<Volume>() == 88);
    assert!(core::mem::size_of::<Entry>() == 840);
    assert!(core::mem::size_of::<Result>() == 16);
};
