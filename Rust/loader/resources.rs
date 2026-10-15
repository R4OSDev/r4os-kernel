//! Canonical R4M0 resource metadata parser; payload bytes remain in the file.
use core::ffi::c_void;

pub const CAPACITY: usize = 64;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Record {
    pub name: [u8; 63],
    pub name_len: u8,
    pub kind: u16,
    pub index: u16,
    pub offset: u32,
    pub bytes: u32,
}
impl Record {
    const EMPTY: Self = Self {
        name: [0; 63],
        name_len: 0,
        kind: 0,
        index: 0,
        offset: 0,
        bytes: 0,
    };
}
#[repr(C)]
pub struct Catalog {
    pub count: usize,
    pub records: [Record; CAPACITY],
}
#[repr(C)]
pub struct Reader {
    pub context: *mut c_void,
    pub read: unsafe extern "C" fn(*mut c_void, usize, *mut u8, usize) -> bool,
}
const INVALID: i32 = 1;
const IO: i32 = 2;
const fn name_byte(byte: u8) -> bool {
    byte >= 0x20 && byte <= 0x7e && byte != b'/' && byte != b'\\' && byte != b':'
}
fn valid_name(name: &[u8]) -> bool {
    !name.is_empty() && name.len() <= 63 && name.iter().copied().all(name_byte)
}
unsafe fn read(reader: &Reader, length: usize, offset: usize, out: &mut [u8]) -> Result<(), i32> {
    if offset > length || out.len() > length - offset {
        return Err(INVALID);
    }
    if !out.is_empty()
        && !unsafe { (reader.read)(reader.context, offset, out.as_mut_ptr(), out.len()) }
    {
        return Err(IO);
    }
    Ok(())
}

unsafe fn parse(catalog: *mut Catalog, reader: &Reader, length: usize) -> Result<(), i32> {
    unsafe { (*catalog).count = 0 };
    let mut count_bytes = [0; 4];
    unsafe { read(reader, length, 0, &mut count_bytes)? };
    let count = u32::from_le_bytes(count_bytes) as usize;
    if count == 0 || count > CAPACITY {
        return Err(INVALID);
    }
    let mut names_end = 4 + 16 * count;
    if names_end > length {
        return Err(INVALID);
    }
    let mut last_kind = 1;
    let mut next_icon = 0;
    let mut help_seen = false;
    for i in 0..count {
        let mut raw = [0; 16];
        unsafe { read(reader, length, 4 + 16 * i, &mut raw)? };
        let kind = u16::from_le_bytes(raw[..2].try_into().unwrap());
        let index = u16::from_le_bytes(raw[2..4].try_into().unwrap());
        let name_offset = u32::from_le_bytes(raw[4..8].try_into().unwrap()) as usize;
        let mut record = Record {
            kind,
            index,
            offset: u32::from_le_bytes(raw[8..12].try_into().unwrap()),
            bytes: u32::from_le_bytes(raw[12..16].try_into().unwrap()),
            ..Record::EMPTY
        };
        // Preserve the original partial-record view, while count remains0
        // until the complete metadata/padding layout has been validated.
        unsafe { (*catalog).records[i] = record };
        if kind < last_kind || kind > 3 || record.bytes == 0 {
            return Err(INVALID);
        }
        last_kind = kind;
        match kind {
            1 => {
                if index != next_icon || name_offset != 0 {
                    return Err(INVALID);
                }
                next_icon += 1;
            }
            2 => {
                if help_seen || index != 0 || name_offset != 0 {
                    return Err(INVALID);
                }
                help_seen = true;
            }
            3 => {
                if index != 0 || name_offset != names_end {
                    return Err(INVALID);
                }
                loop {
                    let mut byte = [0];
                    unsafe { read(reader, length, names_end, &mut byte)? };
                    names_end += 1;
                    if byte[0] == 0 {
                        break;
                    }
                    if !name_byte(byte[0]) || usize::from(record.name_len) == record.name.len() {
                        return Err(INVALID);
                    }
                    record.name[usize::from(record.name_len)] = byte[0];
                    record.name_len += 1;
                    unsafe { (*catalog).records[i] = record };
                }
                if record.name_len == 0 {
                    return Err(INVALID);
                }
                for previous_index in 0..i {
                    let previous = unsafe { (*catalog).records[previous_index] };
                    if previous.kind == 3
                        && previous.name[..usize::from(previous.name_len)]
                            .eq_ignore_ascii_case(&record.name[..usize::from(record.name_len)])
                    {
                        return Err(INVALID);
                    }
                }
            }
            _ => return Err(INVALID),
        }
    }
    let mut cursor = names_end;
    for i in 0..count {
        let record = unsafe { (*catalog).records[i] };
        let padding = (16 - (cursor & 15)) & 15;
        let mut zeros = [0; 15];
        unsafe { read(reader, length, cursor, &mut zeros[..padding])? };
        if zeros[..padding].iter().any(|byte| *byte != 0) {
            return Err(INVALID);
        }
        cursor += padding;
        if record.offset as usize != cursor
            || cursor > length
            || record.bytes as usize > length - cursor
        {
            return Err(INVALID);
        }
        cursor += record.bytes as usize;
    }
    if cursor != length {
        return Err(INVALID);
    }
    unsafe { (*catalog).count = count };
    Ok(())
}

/// # Safety
/// Catalog and callback context stay resident until return. The caller owns
/// this catalog exclusively; callbacks may wait but must not alias it. No
/// exclusive Rust catalog reference is held across a waiting callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_module_resource_parse(
    catalog: *mut Catalog,
    reader: *const Reader,
    length: usize,
) -> i32 {
    unsafe { parse(catalog, &*reader, length) }.map_or_else(|error| error, |()| 0)
}
/// # Safety
/// Name bytes are readable for length bytes and remain resident for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_module_resource_name(name: *const u8, length: usize) -> bool {
    if length == 0 || length > 63 {
        return false;
    }
    valid_name(unsafe { core::slice::from_raw_parts(name, length) })
}
/// # Safety
/// Catalog is an initialized coherent snapshot; name remains readable and
/// resident. No mutation/callback occurs; the result is a bounded index.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_module_resource_find(
    catalog: *const Catalog,
    name: *const u8,
    length: usize,
) -> usize {
    if length == 0 || length > 63 {
        return usize::MAX;
    }
    let name = unsafe { core::slice::from_raw_parts(name, length) };
    if !valid_name(name) {
        return usize::MAX;
    }
    let catalog = unsafe { &*catalog };
    for (i, record) in catalog.records[..catalog.count].iter().enumerate() {
        if record.kind == 3
            && name.eq_ignore_ascii_case(&record.name[..usize::from(record.name_len)])
        {
            return i;
        }
    }
    usize::MAX
}
const _: () = {
    assert!(core::mem::size_of::<Record>() == 76);
    assert!(core::mem::align_of::<Record>() == 4);
    assert!(core::mem::offset_of!(Record, name) == 0);
    assert!(core::mem::offset_of!(Record, name_len) == 63);
    assert!(core::mem::offset_of!(Record, kind) == 64);
    assert!(core::mem::offset_of!(Record, index) == 66);
    assert!(core::mem::offset_of!(Record, offset) == 68);
    assert!(core::mem::offset_of!(Record, bytes) == 72);
    assert!(core::mem::size_of::<Catalog>() == 4872);
    assert!(core::mem::align_of::<Catalog>() == 8);
    assert!(core::mem::offset_of!(Catalog, records) == 8);
    assert!(core::mem::size_of::<Reader>() == 16);
};
