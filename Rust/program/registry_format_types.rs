//! Private C values for the unchanged R4R1 file format. Input/scratch buffers
//! stay with their caller; no language-specific slice crosses the C boundary.
use core::ffi::c_void;
pub const HEADER_SIZE: usize = 96;
pub const KEY_SIZE: usize = 32;
pub const VALUE_SIZE: usize = 32;
pub const INVALID: u32 = u32::MAX;
#[repr(u32)]
#[derive(Clone, Copy, Debug)]
pub enum Error {
    FileTooSmall = 1,
    BadMagic,
    BadVersion,
    BadHeaderSize,
    BadEndian,
    BadHiveKind,
    BadFlags,
    BadFileSize,
    BadChecksumType,
    BadChecksum,
    BadReserved,
    BadRange,
    BadRootKey,
    BadKey,
    BadValue,
    BadName,
    BadData,
    DuplicateKey,
    DuplicateValue,
    RootMismatch,
    InvalidPath,
    OutOfMemory,
    TooManyEntries,
}
pub type Result<T> = core::result::Result<T, Error>;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Bytes {
    pub data: *const u8,
    pub len: usize,
}
impl Bytes {
    pub const EMPTY: Self = Self {
        data: core::ptr::null(),
        len: 0,
    };
    pub fn from_slice(value: &[u8]) -> Self {
        Self {
            data: value.as_ptr(),
            len: value.len(),
        }
    }
    /// # Safety
    /// Caller retains the readable, immutable buffer for the chosen bounded
    /// borrow. An empty span may be null and is never dereferenced.
    pub unsafe fn slice<'a>(self) -> &'a [u8] {
        if self.len == 0 {
            &[]
        } else {
            unsafe { core::slice::from_raw_parts(self.data, self.len) }
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Header {
    pub hive_kind: u16,
    pub generation: u64,
    pub key_table_offset: u32,
    pub key_count: u32,
    pub value_table_offset: u32,
    pub value_count: u32,
    pub string_heap_offset: u32,
    pub string_heap_size: u32,
    pub data_heap_offset: u32,
    pub data_heap_size: u32,
    pub checksum_type: u32,
    pub checksum: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct KeyRecord {
    pub parent_index: u32,
    pub name_offset: u32,
    pub name_len: u16,
    pub first_value_index: u32,
    pub value_count: u32,
    pub first_child_index: u32,
    pub child_count: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ValueRecord {
    pub owner_key_index: u32,
    pub name_offset: u32,
    pub name_len: u16,
    pub value_type: u16,
    pub data_offset: u32,
    pub data_len: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct View {
    pub bytes: Bytes,
    pub header: Header,
    pub key_table: Bytes,
    pub value_table: Bytes,
    pub string_heap: Bytes,
    pub data_heap: Bytes,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Value {
    pub value_type: u16,
    pub data: Bytes,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ParsedRoot {
    pub kind: u16,
    pub rest: Bytes,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BuildValue {
    pub key_path: Bytes,
    pub name: Bytes,
    pub value_type: u16,
    pub data: Bytes,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BuildKey {
    pub parent: u32,
    pub name: Bytes,
    pub flat_index: u32,
    pub first_child: u32,
    pub last_child: u32,
    pub next_sibling: u32,
    pub child_count: u32,
    pub value_count: u32,
    pub value_first: u32,
    pub value_cursor: u32,
}
impl BuildKey {
    pub const EMPTY: Self = Self {
        parent: INVALID,
        name: Bytes::EMPTY,
        flat_index: INVALID,
        first_child: INVALID,
        last_child: INVALID,
        next_sibling: INVALID,
        child_count: 0,
        value_count: 0,
        value_first: 0,
        value_cursor: 0,
    };
}
pub type ReadKey = unsafe extern "C" fn(*const c_void, usize, *mut BuildKey);
pub type WriteKey = unsafe extern "C" fn(*mut c_void, usize, *const BuildKey);
pub type ReadValue = unsafe extern "C" fn(*const c_void, usize, *mut BuildValue);
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BuildInput {
    pub output: *mut u8,
    pub output_capacity: usize,
    pub keys: *mut c_void,
    pub key_capacity: usize,
    pub read_key: Option<ReadKey>,
    pub write_key: Option<WriteKey>,
    pub values: *const c_void,
    pub values_len: usize,
    pub read_value: Option<ReadValue>,
    pub owners: *mut u32,
    pub owners_capacity: usize,
    pub flat: *mut u32,
    pub flat_capacity: usize,
    pub grouped: *mut u32,
    pub grouped_capacity: usize,
    pub kind: u16,
    pub generation: u64,
}

// Private C records: every field, size and alignment is checked on both sides.
const _: () = {
    assert!(core::mem::size_of::<Bytes>() == 16);
    assert!(core::mem::align_of::<Bytes>() == 8);
    assert!(core::mem::offset_of!(Bytes, data) == 0);
    assert!(core::mem::offset_of!(Bytes, len) == 8);
    assert!(core::mem::size_of::<Header>() == 56);
    assert!(core::mem::align_of::<Header>() == 8);
    assert!(core::mem::offset_of!(Header, hive_kind) == 0);
    assert!(core::mem::offset_of!(Header, generation) == 8);
    assert!(core::mem::offset_of!(Header, key_table_offset) == 16);
    assert!(core::mem::offset_of!(Header, key_count) == 20);
    assert!(core::mem::offset_of!(Header, value_table_offset) == 24);
    assert!(core::mem::offset_of!(Header, value_count) == 28);
    assert!(core::mem::offset_of!(Header, string_heap_offset) == 32);
    assert!(core::mem::offset_of!(Header, string_heap_size) == 36);
    assert!(core::mem::offset_of!(Header, data_heap_offset) == 40);
    assert!(core::mem::offset_of!(Header, data_heap_size) == 44);
    assert!(core::mem::offset_of!(Header, checksum_type) == 48);
    assert!(core::mem::offset_of!(Header, checksum) == 52);
    assert!(core::mem::size_of::<KeyRecord>() == 28);
    assert!(core::mem::align_of::<KeyRecord>() == 4);
    assert!(core::mem::offset_of!(KeyRecord, parent_index) == 0);
    assert!(core::mem::offset_of!(KeyRecord, name_offset) == 4);
    assert!(core::mem::offset_of!(KeyRecord, name_len) == 8);
    assert!(core::mem::offset_of!(KeyRecord, first_value_index) == 12);
    assert!(core::mem::offset_of!(KeyRecord, value_count) == 16);
    assert!(core::mem::offset_of!(KeyRecord, first_child_index) == 20);
    assert!(core::mem::offset_of!(KeyRecord, child_count) == 24);
    assert!(core::mem::size_of::<ValueRecord>() == 20);
    assert!(core::mem::align_of::<ValueRecord>() == 4);
    assert!(core::mem::offset_of!(ValueRecord, owner_key_index) == 0);
    assert!(core::mem::offset_of!(ValueRecord, name_offset) == 4);
    assert!(core::mem::offset_of!(ValueRecord, name_len) == 8);
    assert!(core::mem::offset_of!(ValueRecord, value_type) == 10);
    assert!(core::mem::offset_of!(ValueRecord, data_offset) == 12);
    assert!(core::mem::offset_of!(ValueRecord, data_len) == 16);
    assert!(core::mem::size_of::<View>() == 136);
    assert!(core::mem::align_of::<View>() == 8);
    assert!(core::mem::offset_of!(View, bytes) == 0);
    assert!(core::mem::offset_of!(View, header) == 16);
    assert!(core::mem::offset_of!(View, key_table) == 72);
    assert!(core::mem::offset_of!(View, value_table) == 88);
    assert!(core::mem::offset_of!(View, string_heap) == 104);
    assert!(core::mem::offset_of!(View, data_heap) == 120);
    assert!(core::mem::size_of::<Value>() == 24);
    assert!(core::mem::align_of::<Value>() == 8);
    assert!(core::mem::offset_of!(Value, value_type) == 0);
    assert!(core::mem::offset_of!(Value, data) == 8);
    assert!(core::mem::size_of::<ParsedRoot>() == 24);
    assert!(core::mem::align_of::<ParsedRoot>() == 8);
    assert!(core::mem::offset_of!(ParsedRoot, kind) == 0);
    assert!(core::mem::offset_of!(ParsedRoot, rest) == 8);
    assert!(core::mem::size_of::<BuildValue>() == 56);
    assert!(core::mem::align_of::<BuildValue>() == 8);
    assert!(core::mem::offset_of!(BuildValue, key_path) == 0);
    assert!(core::mem::offset_of!(BuildValue, name) == 16);
    assert!(core::mem::offset_of!(BuildValue, value_type) == 32);
    assert!(core::mem::offset_of!(BuildValue, data) == 40);
    assert!(core::mem::size_of::<BuildKey>() == 56);
    assert!(core::mem::align_of::<BuildKey>() == 8);
    assert!(core::mem::offset_of!(BuildKey, parent) == 0);
    assert!(core::mem::offset_of!(BuildKey, name) == 8);
    assert!(core::mem::offset_of!(BuildKey, flat_index) == 24);
    assert!(core::mem::offset_of!(BuildKey, first_child) == 28);
    assert!(core::mem::offset_of!(BuildKey, last_child) == 32);
    assert!(core::mem::offset_of!(BuildKey, next_sibling) == 36);
    assert!(core::mem::offset_of!(BuildKey, child_count) == 40);
    assert!(core::mem::offset_of!(BuildKey, value_count) == 44);
    assert!(core::mem::offset_of!(BuildKey, value_first) == 48);
    assert!(core::mem::offset_of!(BuildKey, value_cursor) == 52);
    assert!(core::mem::size_of::<BuildInput>() == 136);
    assert!(core::mem::align_of::<BuildInput>() == 8);
    assert!(core::mem::offset_of!(BuildInput, output) == 0);
    assert!(core::mem::offset_of!(BuildInput, output_capacity) == 8);
    assert!(core::mem::offset_of!(BuildInput, keys) == 16);
    assert!(core::mem::offset_of!(BuildInput, key_capacity) == 24);
    assert!(core::mem::offset_of!(BuildInput, read_key) == 32);
    assert!(core::mem::offset_of!(BuildInput, write_key) == 40);
    assert!(core::mem::offset_of!(BuildInput, values) == 48);
    assert!(core::mem::offset_of!(BuildInput, values_len) == 56);
    assert!(core::mem::offset_of!(BuildInput, read_value) == 64);
    assert!(core::mem::offset_of!(BuildInput, owners) == 72);
    assert!(core::mem::offset_of!(BuildInput, owners_capacity) == 80);
    assert!(core::mem::offset_of!(BuildInput, flat) == 88);
    assert!(core::mem::offset_of!(BuildInput, flat_capacity) == 96);
    assert!(core::mem::offset_of!(BuildInput, grouped) == 104);
    assert!(core::mem::offset_of!(BuildInput, grouped_capacity) == 112);
    assert!(core::mem::offset_of!(BuildInput, kind) == 120);
    assert!(core::mem::offset_of!(BuildInput, generation) == 128);
};
