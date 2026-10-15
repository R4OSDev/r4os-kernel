//! Original R4M0 metadata/table/relocation reader. Wire acceptance and
//! ordered failures stay unchanged; actual file I/O uses the caller cache.
use crate::fs::vfs_types::Bytes;
use core::{ffi::c_void, ptr};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Header {
    pub version: u16,
    pub arch: u16,
    pub kind_raw: u16,
    pub header_size: u16,
    pub flags: u32,
    pub section_off: u32,
    pub section_count: u32,
    pub import_off: u32,
    pub import_count: u32,
    pub export_off: u32,
    pub export_count: u32,
    pub reloc_off: u32,
    pub reloc_count: u32,
    pub entry_off: u32,
    pub entry_count: u32,
    pub meta_off: u32,
    pub meta_size: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Limits {
    pub max_sections: u32,
    pub max_imports: u32,
    pub max_exports: u32,
    pub max_relocations: u32,
    pub require_sections: bool,
    pub require_entries: bool,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ExportRecord {
    pub name_offset: u32,
    pub section: u32,
    pub offset: u32,
    pub version: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SectionRecord {
    pub name: [u8; 8],
    pub flags: u32,
    pub file_off: u32,
    pub file_size: u32,
    pub mem_size: u32,
    pub alignment: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct EntryRecord {
    pub kind: u32,
    pub section: u32,
    pub offset: u32,
    pub flags: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ImportRecord {
    pub module_offset: u32,
    pub symbol_offset: u32,
    pub min_version: u32,
    pub flags: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RelocationRecord {
    pub kind: u32,
    pub patch_section: u32,
    pub patch_offset: u32,
    pub target_section: u32,
    pub target_offset: u32,
    pub addend: i32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Io {
    pub context: *mut c_void,
    pub read: unsafe extern "C" fn(*mut c_void, usize, *mut u8, usize) -> bool,
    pub log: unsafe extern "C" fn(*mut c_void, *const u8, usize),
    pub file_size: usize,
}
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct Span {
    pub offset: usize,
    pub len: usize,
    pub present: bool,
}
#[repr(C)]
pub struct VersionLabels {
    pub module: [u8; 32],
    pub firmware: [u8; 32],
}
unsafe fn view<'a>(b: Bytes) -> &'a [u8] {
    if b.len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(b.data, b.len) }
    }
}
fn le16(b: &[u8]) -> u16 {
    u16::from_le_bytes(b[..2].try_into().unwrap())
}
fn le32(b: &[u8]) -> u32 {
    u32::from_le_bytes(b[..4].try_into().unwrap())
}
unsafe fn read(io: &Io, offset: usize, out: &mut [u8]) -> bool {
    unsafe { (io.read)(io.context, offset, out.as_mut_ptr(), out.len()) }
}
fn reject(io: &Io, reason: &[u8]) -> bool {
    unsafe { (io.log)(io.context, reason.as_ptr(), reason.len()) };
    false
}
fn range(size: usize, offset: usize, len: usize) -> bool {
    offset <= size && len <= size - offset
}
fn table(size: usize, offset: u32, count: u32, entry_size: usize, required: bool) -> bool {
    if count == 0 {
        return !required;
    }
    if offset == 0 {
        return false;
    }
    let count = count as usize;
    count <= size / entry_size && range(size, offset as usize, count * entry_size)
}
pub(crate) fn header(raw: &[u8]) -> Header {
    Header {
        version: le16(&raw[4..6]),
        arch: le16(&raw[6..8]),
        kind_raw: le16(&raw[8..10]),
        header_size: le16(&raw[10..12]),
        flags: le32(&raw[12..16]),
        section_off: le32(&raw[16..20]),
        section_count: le32(&raw[20..24]),
        import_off: le32(&raw[24..28]),
        import_count: le32(&raw[28..32]),
        export_off: le32(&raw[32..36]),
        export_count: le32(&raw[36..40]),
        reloc_off: le32(&raw[40..44]),
        reloc_count: le32(&raw[44..48]),
        entry_off: le32(&raw[48..52]),
        entry_count: le32(&raw[52..56]),
        meta_off: le32(&raw[56..60]),
        meta_size: le32(&raw[60..64]),
    }
}
fn validate(io: &Io, h: Header, expected: u16, limits: Limits) -> bool {
    if h.version != 1 {
        return reject(io, b"version");
    }
    if h.arch != 1 {
        return reject(io, b"arch");
    }
    if h.header_size != 64 {
        return reject(io, b"header-size");
    }
    let kind = r4os_rust_r4m_kind(h.kind_raw);
    if kind == 0 {
        return reject(io, b"kind");
    }
    if !r4os_rust_r4m_loadable(kind) {
        return reject(io, b"reserved-kind");
    }
    if expected != 0 && kind != expected {
        return reject(io, b"unexpected-kind");
    }
    if h.section_count > limits.max_sections {
        return reject(io, b"section-count");
    }
    if h.import_count > limits.max_imports {
        return reject(io, b"import-count");
    }
    if h.export_count > limits.max_exports {
        return reject(io, b"export-count");
    }
    if h.reloc_count > limits.max_relocations {
        return reject(io, b"reloc-count");
    }
    if !table(
        io.file_size,
        h.section_off,
        h.section_count,
        32,
        limits.require_sections,
    ) {
        return reject(io, b"section-table");
    }
    if !table(
        io.file_size,
        h.entry_off,
        h.entry_count,
        16,
        limits.require_entries,
    ) {
        return reject(io, b"entry-table");
    }
    if !table(io.file_size, h.import_off, h.import_count, 16, false) {
        return reject(io, b"import-table");
    }
    if !table(io.file_size, h.export_off, h.export_count, 16, false) {
        return reject(io, b"export-table");
    }
    if !table(io.file_size, h.reloc_off, h.reloc_count, 24, false) {
        return reject(io, b"reloc-table");
    }
    if h.meta_size != 0 && !range(io.file_size, h.meta_off as usize, h.meta_size as usize) {
        return reject(io, b"metadata");
    }
    true
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_r4m_kind(raw: u16) -> u16 {
    if (1..=6).contains(&raw) { raw } else { 0 }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_r4m_loadable(kind: u16) -> bool {
    (1..=4).contains(&kind)
}
/// # Safety
/// Module bytes remain readable for their nonzero length; no mutation/callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4m_import(module: Bytes, flags: u32) -> bool {
    if flags & !1 != 0 {
        return false;
    }
    if flags == 0 {
        return true;
    }
    let module = unsafe { view(module) };
    ![
        b"R4SYS".as_slice(),
        b"R4DESK",
        b"R4DRAW",
        b"R4NET",
        b"R4AUDIO",
        b"R4DEV",
    ]
    .iter()
    .any(|p| module.eq_ignore_ascii_case(p))
}
/// # Safety
/// Immutable resident callback record and exclusive aligned Header output.
/// Callbacks may wait; no exclusive output borrow spans a callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4m_header(
    io: *const Io,
    expected: u16,
    limits: *const Limits,
    out: *mut Header,
) -> bool {
    let io = unsafe { &*io };
    if io.file_size < 64 {
        return reject(io, b"short-header");
    }
    let mut raw = [0; 64];
    if !unsafe { read(io, 0, &mut raw) } {
        return false;
    }
    if [&b"R4X0"[..], &b"R4D0"[..], &b"R4P0"[..]].contains(&&raw[..4]) {
        return reject(io, b"legacy-format");
    }
    if &raw[..4] != b"R4M0" {
        return reject(io, b"bad-magic");
    }
    let h = header(&raw);
    if !validate(io, h, expected, unsafe { limits.read() }) {
        return false;
    }
    unsafe { out.write(h) };
    true
}
unsafe fn record(io: &Io, h: Header, index: usize, op: u8, out: *mut c_void) -> bool {
    let (off, count, size) = match op {
        0 => (h.section_off, h.section_count, 32),
        1 => (h.entry_off, h.entry_count, 16),
        2 => (h.import_off, h.import_count, 16),
        3 => (h.export_off, h.export_count, 16),
        4 => (h.reloc_off, h.reloc_count, 24),
        _ => unreachable!(),
    };
    if index >= count as usize {
        return false;
    }
    let offset = off as usize + index * size;
    let mut raw = [0; 32];
    if !unsafe { read(io, offset, &mut raw[..size]) } {
        return false;
    }
    unsafe {
        match op {
            0 => out.cast::<SectionRecord>().write(SectionRecord {
                name: raw[..8].try_into().unwrap(),
                flags: le32(&raw[8..12]),
                file_off: le32(&raw[12..16]),
                file_size: le32(&raw[16..20]),
                mem_size: le32(&raw[20..24]),
                alignment: le32(&raw[24..28]),
            }),
            1 => out.cast::<EntryRecord>().write(EntryRecord {
                kind: le32(&raw[..4]),
                section: le32(&raw[4..8]),
                offset: le32(&raw[8..12]),
                flags: le32(&raw[12..16]),
            }),
            2 => out.cast::<ImportRecord>().write(ImportRecord {
                module_offset: le32(&raw[..4]),
                symbol_offset: le32(&raw[4..8]),
                min_version: le32(&raw[8..12]),
                flags: le32(&raw[12..16]),
            }),
            3 => out.cast::<ExportRecord>().write(ExportRecord {
                name_offset: le32(&raw[..4]),
                section: le32(&raw[4..8]),
                offset: le32(&raw[8..12]),
                version: le32(&raw[12..16]),
            }),
            4 => out.cast::<RelocationRecord>().write(relocation(&raw[..24])),
            _ => unreachable!(),
        }
    };
    true
}
fn relocation(raw: &[u8]) -> RelocationRecord {
    RelocationRecord {
        kind: le32(&raw[..4]),
        patch_section: le32(&raw[4..8]),
        patch_offset: le32(&raw[8..12]),
        target_section: le32(&raw[12..16]),
        target_offset: le32(&raw[16..20]),
        addend: le32(&raw[20..24]) as i32,
    }
}
/// # Safety
/// Resident immutable I/O/header, output aligned for selected record0..4.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4m_record(
    io: *const Io,
    h: *const Header,
    index: usize,
    op: u8,
    out: *mut c_void,
) -> bool {
    unsafe { record(&*io, h.read(), index, op, out) }
}
/// # Safety
/// Original 24 readable relocation bytes, aligned exclusive output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4m_decode(
    raw: *const u8,
    len: usize,
    out: *mut RelocationRecord,
) {
    assert!(len >= 24);
    unsafe { out.write(relocation(core::slice::from_raw_parts(raw, 24))) };
}
unsafe fn zstring(io: &Io, offset: u32, out: *mut u8, len: usize) -> Span {
    let offset = offset as usize;
    if offset >= io.file_size || len == 0 {
        return Span::default();
    }
    let wanted = len.min(io.file_size - offset);
    if !unsafe { (io.read)(io.context, offset, out, wanted) } {
        return Span::default();
    }
    let data = unsafe { core::slice::from_raw_parts(out, wanted) };
    if let Some(end) = data.iter().position(|v| *v == 0) {
        return Span {
            offset: 0,
            len: end,
            present: true,
        };
    }
    reject(io, b"zstring-too-large");
    Span::default()
}
/// # Safety
/// Immutable resident I/O; output writable/disjoint for len bytes through wait.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4m_zstring(
    io: *const Io,
    offset: u32,
    out: *mut u8,
    len: usize,
) -> Span {
    unsafe { zstring(&*io, offset, out, len) }
}
/// # Safety
/// Immutable resident I/O/header, disjoint writable caller buffer through wait.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4m_metadata(
    io: *const Io,
    h: *const Header,
    out: *mut u8,
    len: usize,
) -> Span {
    let h = unsafe { h.read() };
    if h.meta_size == 0 {
        return Span {
            offset: 0,
            len: 0,
            present: true,
        };
    }
    let io = unsafe { &*io };
    let size = h.meta_size as usize;
    if size > len {
        reject(io, b"metadata-too-large");
        return Span::default();
    }
    if !unsafe { (io.read)(io.context, h.meta_off as usize, out, size) } {
        return Span::default();
    }
    Span {
        offset: 0,
        len: size,
        present: true,
    }
}
/// # Safety
/// Immutable resident I/O/header/symbol, exclusive scratch through callbacks.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4m_export(
    io: *const Io,
    h: *const Header,
    symbol: Bytes,
    minimum: u32,
    scratch: *mut u8,
    len: usize,
) -> bool {
    let io = unsafe { &*io };
    let h = unsafe { h.read() };
    if h.export_count == 0 {
        return false;
    }
    for index in 0..h.export_count {
        let mut export = core::mem::MaybeUninit::<ExportRecord>::uninit();
        if !unsafe { record(io, h, index as usize, 3, export.as_mut_ptr().cast()) } {
            return false;
        }
        let export = unsafe { export.assume_init() };
        if export.version < minimum {
            continue;
        }
        let span = unsafe { zstring(io, export.name_offset, scratch, len) };
        if !span.present {
            return false;
        }
        if unsafe { core::slice::from_raw_parts(scratch, span.len) == view(symbol) } {
            return true;
        }
    }
    false
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_r4m_windows(count: usize) -> usize {
    if count == 0 {
        0
    } else {
        1 + (count - 1) / (super::file::WINDOW_SIZE / 24)
    }
}
/// # Safety
/// Resident immutable I/O/header and exclusive exact cursor/output. Failed
/// reads leave cursor unchanged; no mutable reference spans a callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4m_next(
    io: *const Io,
    h: *const Header,
    index: *mut usize,
    out: *mut RelocationRecord,
) -> bool {
    let current = unsafe { index.read() };
    if !unsafe { record(&*io, h.read(), current, 4, out.cast()) } {
        return false;
    }
    unsafe { index.write(current + 1) };
    true
}
fn next(meta: &[u8], offset: &mut usize) -> Span {
    while *offset < meta.len() {
        let start = *offset;
        while *offset < meta.len() && meta[*offset] != 0 {
            *offset += 1;
        }
        let end = *offset;
        if *offset < meta.len() {
            *offset += 1;
        }
        if end != start {
            return Span {
                offset: start,
                len: end - start,
                present: true,
            };
        }
    }
    Span::default()
}
fn value(meta: &[u8], prefix: &[u8]) -> Span {
    let mut offset = 0;
    let mut found = Span::default();
    loop {
        let s = next(meta, &mut offset);
        if !s.present {
            return found;
        }
        if meta[s.offset..s.offset + s.len].starts_with(prefix) {
            found = Span {
                offset: s.offset + prefix.len(),
                len: s.len - prefix.len(),
                present: true,
            };
        }
    }
}
/// # Safety
/// Readable metadata/needle; op0 needs exclusive initialized offset. Pure
/// borrowed spans refer only into the caller's original immutable metadata.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4m_meta(
    op: u8,
    meta: Bytes,
    needle: Bytes,
    offset: *mut usize,
) -> Span {
    let meta = unsafe { view(meta) };
    match op {
        0 => next(meta, unsafe { &mut *offset }),
        1 => value(meta, unsafe { view(needle) }),
        2 => {
            let needle = unsafe { view(needle) };
            let mut offset = 0;
            loop {
                let s = next(meta, &mut offset);
                if !s.present {
                    return Span::default();
                }
                if &meta[s.offset..s.offset + s.len] == needle {
                    return s;
                }
            }
        }
        _ => unreachable!(),
    }
}
/// # Safety
/// Immutable readable metadata and aligned writable exclusive labels output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4m_labels(meta: Bytes, out: *mut VersionLabels) {
    let meta = unsafe { view(meta) };
    let mut labels = VersionLabels {
        module: [0; 32],
        firmware: [0; 32],
    };
    for (prefix, label) in [
        (b"module.version=".as_slice(), &mut labels.module),
        (b"firmware.version=".as_slice(), &mut labels.firmware),
    ] {
        let s = value(meta, prefix);
        if !s.present || s.len == 0 || s.len >= 32 {
            continue;
        }
        let bytes = &meta[s.offset..s.offset + s.len];
        if bytes.iter().any(|c| *c < 33 || *c > 126) {
            continue;
        }
        label[..s.len].copy_from_slice(bytes);
    }
    unsafe { ptr::write(out, labels) };
}
const _: () = {
    assert!(core::mem::size_of::<Header>() == 60);
    assert!(core::mem::align_of::<Header>() == 4);
    assert!(core::mem::size_of::<Limits>() == 20);
    assert!(core::mem::size_of::<SectionRecord>() == 28);
    assert!(core::mem::size_of::<EntryRecord>() == 16);
    assert!(core::mem::size_of::<ImportRecord>() == 16);
    assert!(core::mem::size_of::<ExportRecord>() == 16);
    assert!(core::mem::size_of::<RelocationRecord>() == 24);
    assert!(core::mem::size_of::<Span>() == 24);
    assert!(core::mem::size_of::<Io>() == 32);
    assert!(core::mem::size_of::<VersionLabels>() == 64);
};
const _: () = {
    assert!(core::mem::offset_of!(Header, version) == 0);
    assert!(core::mem::offset_of!(Header, arch) == 2);
    assert!(core::mem::offset_of!(Header, kind_raw) == 4);
    assert!(core::mem::offset_of!(Header, header_size) == 6);
    assert!(core::mem::offset_of!(Header, flags) == 8);
    assert!(core::mem::offset_of!(Header, section_off) == 12);
    assert!(core::mem::offset_of!(Header, section_count) == 16);
    assert!(core::mem::offset_of!(Header, import_off) == 20);
    assert!(core::mem::offset_of!(Header, import_count) == 24);
    assert!(core::mem::offset_of!(Header, export_off) == 28);
    assert!(core::mem::offset_of!(Header, export_count) == 32);
    assert!(core::mem::offset_of!(Header, reloc_off) == 36);
    assert!(core::mem::offset_of!(Header, reloc_count) == 40);
    assert!(core::mem::offset_of!(Header, entry_off) == 44);
    assert!(core::mem::offset_of!(Header, entry_count) == 48);
    assert!(core::mem::offset_of!(Header, meta_off) == 52);
    assert!(core::mem::offset_of!(Header, meta_size) == 56);
    assert!(core::mem::offset_of!(Limits, max_sections) == 0);
    assert!(core::mem::offset_of!(Limits, max_imports) == 4);
    assert!(core::mem::offset_of!(Limits, max_exports) == 8);
    assert!(core::mem::offset_of!(Limits, max_relocations) == 12);
    assert!(core::mem::offset_of!(Limits, require_sections) == 16);
    assert!(core::mem::offset_of!(Limits, require_entries) == 17);
    assert!(core::mem::offset_of!(ExportRecord, name_offset) == 0);
    assert!(core::mem::offset_of!(ExportRecord, section) == 4);
    assert!(core::mem::offset_of!(ExportRecord, offset) == 8);
    assert!(core::mem::offset_of!(ExportRecord, version) == 12);
    assert!(core::mem::offset_of!(SectionRecord, name) == 0);
    assert!(core::mem::offset_of!(SectionRecord, flags) == 8);
    assert!(core::mem::offset_of!(SectionRecord, file_off) == 12);
    assert!(core::mem::offset_of!(SectionRecord, file_size) == 16);
    assert!(core::mem::offset_of!(SectionRecord, mem_size) == 20);
    assert!(core::mem::offset_of!(SectionRecord, alignment) == 24);
    assert!(core::mem::offset_of!(EntryRecord, kind) == 0);
    assert!(core::mem::offset_of!(EntryRecord, section) == 4);
    assert!(core::mem::offset_of!(EntryRecord, offset) == 8);
    assert!(core::mem::offset_of!(EntryRecord, flags) == 12);
    assert!(core::mem::offset_of!(ImportRecord, module_offset) == 0);
    assert!(core::mem::offset_of!(ImportRecord, symbol_offset) == 4);
    assert!(core::mem::offset_of!(ImportRecord, min_version) == 8);
    assert!(core::mem::offset_of!(ImportRecord, flags) == 12);
    assert!(core::mem::offset_of!(RelocationRecord, kind) == 0);
    assert!(core::mem::offset_of!(RelocationRecord, patch_section) == 4);
    assert!(core::mem::offset_of!(RelocationRecord, patch_offset) == 8);
    assert!(core::mem::offset_of!(RelocationRecord, target_section) == 12);
    assert!(core::mem::offset_of!(RelocationRecord, target_offset) == 16);
    assert!(core::mem::offset_of!(RelocationRecord, addend) == 20);
};
