//! Kernel NTFS adapter values; existing public Zig values are projected.
use super::ntfs_format_types::{Buffer, View};
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Mounted {
    pub state_slot: u8,
    pub cluster_bytes: u32,
    pub total_sectors: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Entry {
    pub name: [u8; 768],
    pub name_len: usize,
    pub attr: u8,
    pub reparse: bool,
    pub record: u64,
    pub sequence: u16,
    pub size: u64,
    pub created_time: u16,
    pub created_date: u16,
    pub access_date: u16,
    pub modified_time: u16,
    pub modified_date: u16,
}
impl Entry {
    pub const EMPTY: Self = Self {
        name: [0; 768],
        name_len: 0,
        attr: 0,
        reparse: false,
        record: 0,
        sequence: 0,
        size: 0,
        created_time: 0,
        created_date: 0,
        access_date: 0,
        modified_time: 0,
        modified_date: 0,
    };
    pub fn is_dir(self) -> bool {
        self.attr & 16 != 0
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Args {
    pub volume: Mounted,
    pub entry: *const Entry,
    pub buffer: Buffer,
    pub name: View,
    pub other: View,
    pub third: View,
    pub number: u64,
    pub second: u64,
    pub count: u64,
    pub device: usize,
    pub state: *mut core::ffi::c_void,
    pub flag: bool,
    pub read_guard: *mut super::request::Guard,
    pub read_reference: crate::storage::access_state::Reference,
}
