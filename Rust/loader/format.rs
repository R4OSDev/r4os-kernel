//! Original module-registry byte admission, image layout, relocations and R4L
//! table validation. Registry claims and publication migrate separately.
//! Private callers supply bounded, resident C records; outputs are initialized
//! only as far as the original Zig operation wrote them.
use super::r4m::{self, Header, RelocationRecord};
use crate::fs::vfs_types::Bytes;
use core::{ptr, slice};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Section {
    pub name: [u8; 8],
    pub name_len: usize,
    pub flags: u32,
    pub file_off: u32,
    pub file_size: u32,
    pub mem_size: u32,
    pub alignment: u32,
}
#[repr(C)]
pub struct Import {
    pub module: Bytes,
    pub symbol: Bytes,
    pub min_version: u32,
    pub flags: u32,
}
#[repr(C)]
pub struct ResolvedImport {
    pub module: [u8; 32],
    pub module_len: usize,
    pub symbol: [u8; 32],
    pub symbol_len: usize,
    pub version: u32,
    pub address: u64,
}
#[repr(C)]
pub struct RelocationImage {
    pub sections: *const Section,
    pub section_count: usize,
    pub offsets: *const usize,
    pub offset_count: usize,
    pub image: *mut u8,
    pub image_len: usize,
    pub imports: *const ResolvedImport,
    pub import_count: usize,
}
#[repr(C)]
pub struct LoadedExport {
    pub name: Bytes,
    pub version: u32,
    pub section_index: u32,
    pub export_offset: u32,
    pub sections: *const Section,
    pub section_count: usize,
    pub offsets: *const usize,
    pub offset_count: usize,
    pub image: Bytes,
}
unsafe fn view<'a>(b: Bytes) -> &'a [u8] {
    if b.len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(b.data, b.len) }
    }
}
unsafe fn records<'a, T>(p: *const T, n: usize) -> &'a [T] {
    if n == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(p, n) }
    }
}
fn le16(b: &[u8]) -> u16 {
    u16::from_le_bytes(b[..2].try_into().unwrap())
}
fn le32(b: &[u8]) -> u32 {
    u32::from_le_bytes(b[..4].try_into().unwrap())
}
fn le64(b: &[u8]) -> u64 {
    u64::from_le_bytes(b[..8].try_into().unwrap())
}
pub(crate) fn range(size: usize, offset: u32, len: u32) -> bool {
    (offset as usize) <= size && (len as usize) <= size - offset as usize
}
fn table(size: usize, offset: u32, count: u32, width: usize, required: bool) -> bool {
    if count == 0 {
        return !required;
    }
    if offset == 0 {
        return false;
    }
    let bytes = u64::from(count) * width as u64;
    bytes <= u64::from(u32::MAX) && range(size, offset, bytes as u32)
}
pub(crate) fn zstring(bytes: &[u8], offset: u32) -> Option<&[u8]> {
    let data = bytes.get(offset as usize..)?;
    if data.is_empty() {
        return None;
    }
    let len = data.iter().position(|b| *b == 0)?;
    Some(&data[..len])
}
fn names_equal(a: &[u8], b: &[u8]) -> bool {
    a.eq_ignore_ascii_case(b)
}
fn valid_module(name: &[u8], dot: bool) -> bool {
    !name.is_empty()
        && name.len() < 32
        && name
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-' || (dot && *b == b'.'))
}
fn valid_symbol(name: &[u8]) -> bool {
    !name.is_empty()
        && name.len() < 32
        && (name[0].is_ascii_alphabetic() || name[0] == b'_')
        && name[1..]
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}
fn prefix(name: &[u8]) -> bool {
    name.len() >= 5 && names_equal(&name[..5], b"API_V")
}
fn major(name: &[u8]) -> Option<u16> {
    if !prefix(name) || name.len() == 5 || name[5] == b'0' {
        return None;
    }
    let mut value = 0u32;
    for byte in &name[5..] {
        if !byte.is_ascii_digit() {
            return None;
        }
        let digit = u32::from(*byte - b'0');
        if value > (0xffff - digit) / 10 {
            return None;
        }
        value = value * 10 + digit;
    }
    Some(value as u16)
}
fn entries(header: &Header, bytes: &[u8]) -> bool {
    if header.entry_count == 0 {
        return false;
    }
    let sizes = section_sizes(header, bytes);
    for i in 0..header.entry_count as usize {
        let off = header.entry_off as usize + i * 16;
        let section = le32(&bytes[off + 4..]);
        if section >= header.section_count || le32(&bytes[off + 8..]) >= sizes[section as usize] {
            return false;
        }
    }
    true
}
fn section_sizes(header: &Header, bytes: &[u8]) -> [u32; 16] {
    let mut sizes = [0; 16];
    for (i, size) in sizes[..header.section_count as usize]
        .iter_mut()
        .enumerate()
    {
        let off = header.section_off as usize + i * 32;
        *size = if le32(&bytes[off + 8..]) & 1 == 0 {
            0
        } else {
            le32(&bytes[off + 20..])
        };
    }
    sizes
}
fn imports(header: &Header, bytes: &[u8]) -> bool {
    for i in 0..header.import_count as usize {
        let off = header.import_off as usize + i * 16;
        let Some(module) = zstring(bytes, le32(&bytes[off..])) else {
            return false;
        };
        let Some(symbol) = zstring(bytes, le32(&bytes[off + 4..])) else {
            return false;
        };
        if !valid_module(module, true) || !valid_symbol(symbol) || le32(&bytes[off + 8..]) == 0 {
            return false;
        }
        let module = Bytes {
            data: module.as_ptr(),
            len: module.len(),
        };
        if !unsafe { r4m::r4os_rust_r4m_import(module, le32(&bytes[off + 12..])) } {
            return false;
        }
    }
    true
}
fn exports(header: &Header, bytes: &[u8]) -> bool {
    let sizes = section_sizes(header, bytes);
    for i in 0..header.export_count as usize {
        let off = header.export_off as usize + i * 16;
        let section = le32(&bytes[off + 4..]);
        let offset = le32(&bytes[off + 8..]);
        let Some(name) = zstring(bytes, le32(&bytes[off..])) else {
            return false;
        };
        if !valid_symbol(name)
            || le32(&bytes[off + 12..]) == 0
            || section >= header.section_count
            || offset >= sizes[section as usize]
        {
            return false;
        }
        for prior in 0..i {
            let prior_off = header.export_off as usize + prior * 16;
            let Some(prior_name) = zstring(bytes, le32(&bytes[prior_off..])) else {
                return false;
            };
            if names_equal(name, prior_name) {
                return false;
            }
        }
    }
    true
}
fn tables(h: &Header, b: &[u8]) -> bool {
    if h.version != 1
        || h.arch != 1
        || h.header_size != 64
        || h.section_count == 0
        || h.section_count > 16
        || h.import_count > 16
        || h.export_count > 16
    {
        return false;
    }
    if !table(b.len(), h.section_off, h.section_count, 32, true)
        || !table(b.len(), h.entry_off, h.entry_count, 16, true)
        || !table(b.len(), h.import_off, h.import_count, 16, false)
        || !table(b.len(), h.export_off, h.export_count, 16, false)
        || !table(b.len(), h.reloc_off, h.reloc_count, 24, false)
        || (h.meta_size != 0 && !range(b.len(), h.meta_off, h.meta_size))
    {
        return false;
    }
    entries(h, b) && imports(h, b) && exports(h, b)
}
fn section_kinds(sections: &[Section], kind: u16) -> bool {
    let mut resources = false;
    for section in sections {
        let resource = &section.name[..section.name_len] == b".rsrc";
        if section.flags & 1 != 0 {
            if resource {
                return false;
            }
            continue;
        }
        if kind == 2
            || resources
            || !resource
            || section.flags != 0
            || section.alignment != 16
            || section.file_off & 15 != 0
            || section.file_size == 0
            || section.mem_size != section.file_size
        {
            return false;
        }
        resources = true;
    }
    true
}

/// # Safety
/// Input is resident for this call; out is aligned writable Header storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_header(bytes: Bytes, out: *mut Header) -> bool {
    let b = unsafe { view(bytes) };
    if b.len() < 64 || &b[..4] != b"R4M0" {
        return false;
    }
    unsafe { ptr::write(out, r4m::header(b)) };
    true
}
/// # Safety
/// For individual subtable operations the caller has already bounded all
/// referenced tables and section_count, as in the original internal helpers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_validate(bytes: Bytes, h: &Header, op: u8) -> bool {
    let b = unsafe { view(bytes) };
    match op {
        0 => tables(h, b),
        1 => entries(h, b),
        2 => imports(h, b),
        3 => exports(h, b),
        _ => false,
    }
}
/// # Safety
/// The validated section table is resident; out provides len aligned slots.
/// Failure preserves the original partial-output prefix.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_sections(
    bytes: Bytes,
    h: &Header,
    out: *mut Section,
    len: usize,
) -> bool {
    if h.section_count as usize > len {
        return false;
    }
    let b = unsafe { view(bytes) };
    for i in 0..h.section_count as usize {
        let off = h.section_off as usize + i * 32;
        let name: [u8; 8] = b[off..off + 8].try_into().unwrap();
        let name_len = name.iter().position(|c| *c == 0).unwrap_or(8);
        let file_off = le32(&b[off + 12..]);
        let file_size = le32(&b[off + 16..]);
        let mem_size = le32(&b[off + 20..]);
        let alignment = le32(&b[off + 24..]);
        if name_len == 0
            || mem_size < file_size
            || !alignment.is_power_of_two()
            || (file_size != 0 && !range(b.len(), file_off, file_size))
        {
            return false;
        }
        unsafe {
            ptr::write(
                out.add(i),
                Section {
                    name,
                    name_len,
                    flags: le32(&b[off + 8..]),
                    file_off,
                    file_size,
                    mem_size,
                    alignment,
                },
            )
        };
    }
    if r4m::r4os_rust_r4m_kind(h.kind_raw) == 0 {
        return false;
    }
    section_kinds(
        unsafe { records(out, h.section_count as usize) },
        h.kind_raw,
    )
}
/// # Safety
/// The caller supplies len initialized section records with name_len <=8.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_section_kinds(
    sections: *const Section,
    len: usize,
    kind: u16,
) -> bool {
    section_kinds(unsafe { records(sections, len) }, kind)
}
/// # Safety
/// Sections and offsets have len resident elements; image_size is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_layout(
    sections: *const Section,
    len: usize,
    offsets: *mut usize,
    image_size: *mut usize,
) -> bool {
    let mut cursor = 0usize;
    for (i, s) in unsafe { records(sections, len) }.iter().enumerate() {
        if s.flags & 1 == 0 {
            unsafe { ptr::write(offsets.add(i), 0) };
            continue;
        }
        let alignment = if s.alignment > 4096 {
            s.alignment as usize
        } else {
            4096
        };
        cursor = (cursor + alignment - 1) & !(alignment - 1);
        unsafe { ptr::write(offsets.add(i), cursor) };
        cursor += s.mem_size as usize;
    }
    let size = (cursor + 4095) & !4095;
    if size == 0 {
        return false;
    }
    unsafe { ptr::write(image_size, size) };
    true
}
fn signed(base: u64, addend: i32) -> Option<u64> {
    if addend >= 0 {
        base.checked_add(addend as u64)
    } else {
        base.checked_sub((-i64::from(addend)) as u64)
    }
}
fn target(
    reloc: RelocationRecord,
    sections: &[Section],
    offsets: &[usize],
    base: u64,
) -> Option<u64> {
    let s = sections.get(reloc.target_section as usize)?;
    if s.flags & 1 == 0 || reloc.target_offset >= s.mem_size {
        return None;
    }
    Some(base + offsets[reloc.target_section as usize] as u64 + u64::from(reloc.target_offset))
}
/// # Safety
/// All image metadata and imports are initialized and immutable for this call;
/// the resident writable image does not alias them. Offsets cover sections.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_reloc(reloc: RelocationRecord, io: &RelocationImage) -> u8 {
    let sections = unsafe { records(io.sections, io.section_count) };
    let offsets = unsafe { records(io.offsets, io.offset_count) };
    let imports = unsafe { records(io.imports, io.import_count) };
    let Some(section) = sections.get(reloc.patch_section as usize) else {
        return 2;
    };
    let width = match reloc.kind {
        2 => 4,
        1 | 3 | 4 => 8,
        _ => 1,
    };
    if section.flags & 1 == 0
        || reloc.patch_offset > section.mem_size
        || width > section.mem_size - reloc.patch_offset
    {
        return 2;
    }
    let off = offsets[reloc.patch_section as usize] + reloc.patch_offset as usize;
    if off > io.image_len || width as usize > io.image_len - off {
        return 2;
    }
    let base = io.image as u64;
    let value = match reloc.kind {
        1 | 3 => {
            let Some(address) =
                target(reloc, sections, offsets, base).and_then(|a| signed(a, reloc.addend))
            else {
                return 2;
            };
            address
        }
        2 => {
            let Some(address) =
                target(reloc, sections, offsets, base).and_then(|a| signed(a, reloc.addend))
            else {
                return 2;
            };
            let patch =
                base + offsets[reloc.patch_section as usize] as u64 + u64::from(reloc.patch_offset);
            let delta = i128::from(address) - i128::from(patch + 4);
            if !(-2147483648..=2147483647).contains(&delta) {
                return 2;
            }
            u64::from((delta as i32) as u32)
        }
        4 => {
            let Some(import) = imports.get(reloc.target_section as usize) else {
                return 3;
            };
            if import.address == 0 && reloc.addend != 0 {
                return 2;
            }
            let Some(address) = signed(import.address, reloc.addend) else {
                return 2;
            };
            address
        }
        _ => return 1,
    };
    let bytes = value.to_le_bytes();
    unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), io.image.add(off), width as usize) };
    0
}
/// # Safety
/// The import table is already bounded. Input stays resident; out is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_import(
    h: &Header,
    bytes: Bytes,
    index: usize,
    out: *mut Import,
) -> bool {
    if index >= h.import_count as usize {
        return false;
    }
    let b = unsafe { view(bytes) };
    let off = h.import_off as usize + index * 16;
    let Some(module) = zstring(b, le32(&b[off..])) else {
        return false;
    };
    let Some(symbol) = zstring(b, le32(&b[off + 4..])) else {
        return false;
    };
    unsafe {
        ptr::write(
            out,
            Import {
                module: Bytes {
                    data: module.as_ptr(),
                    len: module.len(),
                },
                symbol: Bytes {
                    data: symbol.as_ptr(),
                    len: symbol.len(),
                },
                min_version: le32(&b[off + 8..]),
                flags: le32(&b[off + 12..]),
            },
        )
    };
    true
}
/// # Safety
/// The relocation table is bounded; out provides initialized-on-success storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_reloc_record(
    h: &Header,
    bytes: Bytes,
    index: usize,
    out: *mut RelocationRecord,
) -> bool {
    if index >= h.reloc_count as usize {
        return false;
    }
    let b = unsafe { view(bytes) };
    let off = h.reloc_off as usize + index * 24;
    let raw = &b[off..off + 24];
    unsafe { r4m::r4os_rust_r4m_decode(raw.as_ptr(), 24, out) };
    true
}
fn loaded_query(table: &[u8], version: u32) -> &'static [u8] {
    if table.len() < 32 {
        return b"query-range";
    }
    let size = le32(&table[8..]);
    if version != 1
        || le32(table) != 0x314c3452
        || le32(&table[4..]) != 1
        || size < 32
        || size & 7 != 0
        || size as usize > table.len()
        || le64(&table[24..]) != 0
    {
        return b"query-header";
    }
    b""
}
fn loaded_interface(table: &[u8], version: u32, major: u16) -> &'static [u8] {
    if table.len() < 32 {
        return b"interface-range";
    }
    let flags = le16(&table[6..]);
    let size = le32(&table[8..]);
    let minor = le16(&table[14..]);
    if version == 0
        || version > 0xffff
        || le32(table) != 0x31493452
        || le16(&table[4..]) != 1
        || flags & 0xff != 0
        || size < 32
        || size & 7 != 0
        || size as usize > table.len()
        || le16(&table[12..]) != major
        || minor == 0
        || u32::from(minor) != version
        || (le64(&table[16..]) == 0 && le64(&table[24..]) == 0)
    {
        return b"interface-header";
    }
    b""
}
unsafe fn loaded_export(io: &LoadedExport) -> &'static [u8] {
    let name = unsafe { view(io.name) };
    let query = names_equal(name, b"Query");
    if !query && !prefix(name) {
        return b"";
    }
    if io.section_index as usize >= io.section_count || io.section_index as usize >= io.offset_count
    {
        return b"section";
    }
    let s = &unsafe { records(io.sections, io.section_count) }[io.section_index as usize];
    if io.export_offset >= s.mem_size {
        return b"offset";
    }
    let runtime = unsafe { records(io.offsets, io.offset_count) }[io.section_index as usize]
        + io.export_offset as usize;
    let available = (s.mem_size - io.export_offset) as usize;
    let image = unsafe { view(io.image) };
    if runtime > image.len() || available > image.len() - runtime {
        return b"range";
    }
    let table = &image[runtime..runtime + available];
    if table.as_ptr() as usize & 7 != 0 {
        return b"alignment";
    }
    if query {
        return loaded_query(table, io.version);
    }
    let Some(major) = major(name) else {
        return b"major-name";
    };
    loaded_interface(table, io.version, major)
}
/// # Safety
/// All referenced tables, lengths, names and resident image are initialized.
/// Returned rejection text is immutable static storage; an empty span accepts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_loaded_export(io: &LoadedExport) -> Bytes {
    let reason = unsafe { loaded_export(io) };
    Bytes {
        data: reason.as_ptr(),
        len: reason.len(),
    }
}
/// # Safety
/// Name bytes are resident for this synchronous, non-callback operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_name(bytes: Bytes, op: u8) -> u16 {
    let name = unsafe { view(bytes) };
    match op {
        0 => u16::from(valid_module(name, false)),
        1 => u16::from(valid_module(name, true)),
        2 => u16::from(valid_symbol(name)),
        3 => u16::from(name.len() >= 4 && names_equal(&name[name.len() - 4..], b".R4L")),
        4 => u16::from(prefix(name)),
        5 => major(name).unwrap_or(0),
        _ => 0,
    }
}
const _: () = {
    use core::mem::{offset_of, size_of};
    assert!(size_of::<Section>() == 40);
    assert!(offset_of!(Section, name) == 0);
    assert!(offset_of!(Section, name_len) == 8);
    assert!(offset_of!(Section, flags) == 16);
    assert!(offset_of!(Section, file_off) == 20);
    assert!(offset_of!(Section, file_size) == 24);
    assert!(offset_of!(Section, mem_size) == 28);
    assert!(offset_of!(Section, alignment) == 32);
    assert!(size_of::<Import>() == 40);
    assert!(offset_of!(Import, module) == 0);
    assert!(offset_of!(Import, symbol) == 16);
    assert!(offset_of!(Import, min_version) == 32);
    assert!(offset_of!(Import, flags) == 36);
    assert!(size_of::<ResolvedImport>() == 96);
    assert!(offset_of!(ResolvedImport, module) == 0);
    assert!(offset_of!(ResolvedImport, module_len) == 32);
    assert!(offset_of!(ResolvedImport, symbol) == 40);
    assert!(offset_of!(ResolvedImport, symbol_len) == 72);
    assert!(offset_of!(ResolvedImport, version) == 80);
    assert!(offset_of!(ResolvedImport, address) == 88);
    assert!(size_of::<RelocationImage>() == 64);
    assert!(offset_of!(RelocationImage, sections) == 0);
    assert!(offset_of!(RelocationImage, section_count) == 8);
    assert!(offset_of!(RelocationImage, offsets) == 16);
    assert!(offset_of!(RelocationImage, offset_count) == 24);
    assert!(offset_of!(RelocationImage, image) == 32);
    assert!(offset_of!(RelocationImage, image_len) == 40);
    assert!(offset_of!(RelocationImage, imports) == 48);
    assert!(offset_of!(RelocationImage, import_count) == 56);
    assert!(size_of::<LoadedExport>() == 80);
    assert!(offset_of!(LoadedExport, name) == 0);
    assert!(offset_of!(LoadedExport, version) == 16);
    assert!(offset_of!(LoadedExport, section_index) == 20);
    assert!(offset_of!(LoadedExport, export_offset) == 24);
    assert!(offset_of!(LoadedExport, sections) == 32);
    assert!(offset_of!(LoadedExport, section_count) == 40);
    assert!(offset_of!(LoadedExport, offsets) == 48);
    assert!(offset_of!(LoadedExport, offset_count) == 56);
    assert!(offset_of!(LoadedExport, image) == 64);
};
