//! Original complete module discovery/dependency/image loader. No registry,
//! reader or table borrow spans heap, filesystem, logger or module callbacks.
use super::{
    file, format as fmt, r4m, registry_control as ctl, registry_queries as query,
    registry_state as state, resources,
};
use crate::{
    fs::{vfs, vfs_types as t},
    memory::scratch::Scratch as ResidentScratch,
};
use core::{
    ffi::c_void,
    mem::{self, MaybeUninit},
    ptr, slice,
};
type Header = r4m::Header;
type Section = fmt::Section;
type Resolved = fmt::ResolvedImport;
const N: usize = 16;
const LIMITS: r4m::Limits = r4m::Limits {
    max_sections: 16,
    max_imports: 16,
    max_exports: 16,
    max_relocations: u32::MAX,
    require_sections: true,
    require_entries: true,
};
fn span(s: &[u8]) -> t::Bytes {
    ctl::span(s)
}
unsafe fn bytes<'a>(s: t::Bytes) -> &'a [u8] {
    unsafe { ctl::bytes(s) }
}
fn copy(s: &[u8], out: &mut [u8]) -> usize {
    let n = s.len().min(out.len());
    out[..n].copy_from_slice(&s[..n]);
    n
}
fn name_len(s: &[u8]) -> usize {
    s.iter().position(|v| *v == 0).unwrap_or(s.len())
}
fn fallback(name: &[u8]) -> &[u8] {
    if name.len() > 4 && name[name.len() - 4] == b'.' {
        &name[..name.len() - 4]
    } else {
        name
    }
}
fn report(prefix: &[u8], name: &[u8]) -> bool {
    ctl::log(prefix);
    ctl::log(name);
    ctl::log(b"\r\n");
    false
}

struct Reader {
    source: file::Source,
    file_size: usize,
    windows: [file::Window; 2],
    stamp: u64,
}
struct ReadContext {
    source: *const file::Source,
    name: t::Bytes,
    verbose: bool,
}
unsafe extern "C" fn range_read(
    raw: *mut c_void,
    offset: usize,
    out: *mut u8,
    len: usize,
) -> file::Output {
    let c = unsafe { &*raw.cast::<ReadContext>() };
    unsafe { file::r4os_rust_module_file_range(c.source, offset, out, len, c.name, c.verbose) }
}
unsafe fn reader_init(reader: *mut Reader, source: *const file::Source) {
    unsafe {
        // Preserve the original source identity directly in its resident
        // scratch field. A by-value 936-byte temporary would stay in every
        // recursive dependency frame on the bounded kernel stack.
        ptr::addr_of_mut!((*reader).file_size).write((*source).entry.size as usize);
        ptr::copy_nonoverlapping(source, ptr::addr_of_mut!((*reader).source), 1);
        file::r4os_rust_module_file_reset(
            ptr::addr_of_mut!((*reader).windows).cast(),
            ptr::addr_of_mut!((*reader).stamp),
        );
    }
}
unsafe fn read(
    reader: *mut Reader,
    offset: usize,
    out: *mut u8,
    len: usize,
    name: &[u8],
    verbose: bool,
) -> bool {
    let mut context = ReadContext {
        source: unsafe { ptr::addr_of!((*reader).source) },
        name: span(name),
        verbose,
    };
    let io = file::Io {
        context: ptr::from_mut(&mut context).cast(),
        read: range_read,
        name: context.name,
        verbose,
    };
    unsafe {
        file::r4os_rust_module_file_cached(
            ptr::addr_of_mut!((*reader).windows).cast(),
            ptr::addr_of_mut!((*reader).stamp),
            (*reader).file_size,
            offset,
            out,
            len,
            &io,
        )
    }
}
struct MetaContext {
    reader: *mut Reader,
    name: t::Bytes,
}
unsafe extern "C" fn metadata_read(
    raw: *mut c_void,
    offset: usize,
    out: *mut u8,
    len: usize,
) -> bool {
    let c = unsafe { &*raw.cast::<MetaContext>() };
    unsafe { read(c.reader, offset, out, len, bytes(c.name), true) }
}
unsafe extern "C" fn metadata_log(raw: *mut c_void, reason: *const u8, len: usize) {
    let c = unsafe { &*raw.cast::<MetaContext>() };
    ctl::log(b"[R4M] ");
    ctl::log(unsafe { slice::from_raw_parts(reason, len) });
    ctl::log(b" failed ");
    ctl::log(unsafe { bytes(c.name) });
    ctl::log(b"\r\n");
}
unsafe fn io(reader: *mut Reader, c: &mut MetaContext, name: &[u8]) -> r4m::Io {
    c.reader = reader;
    c.name = span(name);
    r4m::Io {
        context: ptr::from_mut(c).cast(),
        read: metadata_read,
        log: metadata_log,
        file_size: unsafe { (*reader).file_size },
    }
}
unsafe fn header(reader: *mut Reader, kind: u16, name: &[u8]) -> Option<Header> {
    let mut context = MetaContext {
        reader,
        name: span(name),
    };
    let io = unsafe { io(reader, &mut context, name) };
    let mut out = MaybeUninit::<Header>::uninit();
    if unsafe { r4m::r4os_rust_r4m_header(&io, kind, &LIMITS, out.as_mut_ptr()) } {
        Some(unsafe { out.assume_init() })
    } else {
        None
    }
}
unsafe fn record<T>(
    reader: *mut Reader,
    h: &Header,
    index: usize,
    op: u8,
    name: &[u8],
) -> Option<T> {
    let mut context = MetaContext {
        reader,
        name: span(name),
    };
    let io = unsafe { io(reader, &mut context, name) };
    let mut out = MaybeUninit::<T>::uninit();
    if unsafe { r4m::r4os_rust_r4m_record(&io, h, index, op, out.as_mut_ptr().cast()) } {
        Some(unsafe { out.assume_init() })
    } else {
        None
    }
}
unsafe fn string<'a>(
    reader: *mut Reader,
    offset: u32,
    out: &'a mut [u8],
    name: &[u8],
) -> Option<&'a [u8]> {
    let mut context = MetaContext {
        reader,
        name: span(name),
    };
    let io = unsafe { io(reader, &mut context, name) };
    let s = unsafe { r4m::r4os_rust_r4m_zstring(&io, offset, out.as_mut_ptr(), out.len()) };
    if s.present {
        Some(&out[s.offset..s.offset + s.len])
    } else {
        None
    }
}
unsafe fn metadata<'a>(
    reader: *mut Reader,
    h: &Header,
    out: &'a mut [u8],
    name: &[u8],
) -> Option<&'a [u8]> {
    let mut context = MetaContext {
        reader,
        name: span(name),
    };
    let io = unsafe { io(reader, &mut context, name) };
    let s = unsafe { r4m::r4os_rust_r4m_metadata(&io, h, out.as_mut_ptr(), out.len()) };
    if s.present {
        Some(&out[s.offset..s.offset + s.len])
    } else {
        None
    }
}
// The same native bounded reader serves loader-local R4D/R4P metadata
// probes. Allocate its two windows in their final resident location, retaining
// the exact caller task until the last read has returned and release is acked.
#[inline(never)]
pub(crate) unsafe fn metadata_probe(
    source: *const file::Source,
    expected: u16,
    output: &mut [u8],
    name: &[u8],
) -> Option<usize> {
    let reader = ResidentScratch::<Reader>::new()?;
    unsafe { reader_init(reader.as_mut_ptr(), source) };
    let h = unsafe { header(reader.as_mut_ptr(), expected, name) }?;
    unsafe { metadata(reader.as_mut_ptr(), &h, output, name) }.map(|s| s.len())
}
fn first_meta(meta: &[u8]) -> Option<&[u8]> {
    let mut offset = 0;
    let s = unsafe { r4m::r4os_rust_r4m_meta(0, span(meta), span(b""), &mut offset) };
    if s.present {
        Some(&meta[s.offset..s.offset + s.len])
    } else {
        None
    }
}

#[derive(Clone, Copy)]
struct FileImport {
    module: [u8; 32],
    module_len: usize,
    symbol: [u8; 32],
    symbol_len: usize,
    min_version: u32,
    flags: u32,
}
impl FileImport {
    const EMPTY: Self = Self {
        module: [0; 32],
        module_len: 0,
        symbol: [0; 32],
        symbol_len: 0,
        min_version: 0,
        flags: 0,
    };
    fn request(&self) -> fmt::Import {
        fmt::Import {
            module: span(&self.module[..self.module_len]),
            symbol: span(&self.symbol[..self.symbol_len]),
            min_version: self.min_version,
            flags: self.flags,
        }
    }
}
struct Tables {
    labels: r4m::VersionLabels,
    header: Header,
    sections: [MaybeUninit<Section>; N],
    imports: [FileImport; N],
    exports: [query::Export; N],
    name: [u8; 32],
    name_len: usize,
}
struct Scratch {
    reader: Reader,
    tables: Tables,
    resolved: [MaybeUninit<Resolved>; N],
    offsets: [MaybeUninit<usize>; N],
}
unsafe fn scratch(load: &state::Prepared) -> *mut Scratch {
    load.scratch.cast()
}
unsafe fn table_name(table: *const Tables, default: &[u8]) -> &[u8] {
    if unsafe { (*table).name_len } == 0 {
        fallback(default)
    } else {
        unsafe { slice::from_raw_parts(ptr::addr_of!((*table).name).cast(), (*table).name_len) }
    }
}
unsafe fn sections<'a>(table: *const Tables) -> &'a [Section] {
    unsafe {
        slice::from_raw_parts(
            ptr::addr_of!((*table).sections).cast(),
            (*table).header.section_count as usize,
        )
    }
}
unsafe fn tables(reader: *mut Reader, out: *mut Tables, kind: u16, default: &[u8]) -> bool {
    let Some(h) = (unsafe { header(reader, kind, b"r4m-module-header") }) else {
        return false;
    };
    unsafe {
        ptr::addr_of_mut!((*out).header).write(h);
        ptr::addr_of_mut!((*out).labels).write(r4m::VersionLabels {
            module: [0; 32],
            firmware: [0; 32],
        });
        ptr::write_bytes(
            ptr::addr_of_mut!((*out).imports).cast::<u8>(),
            0,
            mem::size_of::<[FileImport; N]>(),
        );
        ptr::write_bytes(
            ptr::addr_of_mut!((*out).exports).cast::<u8>(),
            0,
            mem::size_of::<[query::Export; N]>(),
        );
        ptr::addr_of_mut!((*out).name).write([0; 32]);
        ptr::addr_of_mut!((*out).name_len).write(0);
    }
    for i in 0..h.section_count as usize {
        let Some(r) =
            (unsafe { record::<r4m::SectionRecord>(reader, &h, i, 0, b"r4m-section-table") })
        else {
            return false;
        };
        let n = name_len(&r.name);
        if n == 0
            || r.mem_size < r.file_size
            || !r.alignment.is_power_of_two()
            || (r.file_size != 0
                && !fmt::range(unsafe { (*reader).file_size }, r.file_off, r.file_size))
        {
            return false;
        }
        let s = Section {
            name: r.name,
            name_len: n,
            flags: r.flags,
            file_off: r.file_off,
            file_size: r.file_size,
            mem_size: r.mem_size,
            alignment: r.alignment,
        };
        unsafe {
            (*out).sections[i].write(s);
        }
    }
    let sect = unsafe { sections(out) };
    if !unsafe { fmt::r4os_rust_mod_section_kinds(sect.as_ptr(), sect.len(), h.kind_raw) } {
        return false;
    }
    for i in 0..h.entry_count as usize {
        let Some(r) = (unsafe { record::<r4m::EntryRecord>(reader, &h, i, 1, b"r4m-entry-table") })
        else {
            return false;
        };
        if r.section as usize >= sect.len()
            || sect[r.section as usize].flags & 1 == 0
            || r.offset >= sect[r.section as usize].mem_size
        {
            return false;
        }
    }
    for i in 0..h.import_count as usize {
        let Some(r) =
            (unsafe { record::<r4m::ImportRecord>(reader, &h, i, 2, b"r4m-import-table") })
        else {
            return false;
        };
        let mut module = [0; 128];
        let mut symbol = [0; 128];
        let Some(module) =
            (unsafe { string(reader, r.module_offset, &mut module, b"r4m-import-module") })
        else {
            return false;
        };
        let Some(symbol) =
            (unsafe { string(reader, r.symbol_offset, &mut symbol, b"r4m-import-symbol") })
        else {
            return false;
        };
        if unsafe { fmt::r4os_rust_mod_name(span(module), 1) } == 0
            || unsafe { fmt::r4os_rust_mod_name(span(symbol), 2) } == 0
            || r.min_version == 0
            || !unsafe { r4m::r4os_rust_r4m_import(span(module), r.flags) }
        {
            ctl::log(b"[MOD] malformed import module=");
            ctl::log(module);
            ctl::log(b" symbol=");
            ctl::log(symbol);
            ctl::log(b" importer=");
            ctl::log(default);
            ctl::log(b"\r\n");
            return false;
        }
        let mut v = FileImport::EMPTY;
        v.module_len = copy(module, &mut v.module);
        v.symbol_len = copy(symbol, &mut v.symbol);
        v.min_version = r.min_version;
        v.flags = r.flags;
        unsafe { ptr::addr_of_mut!((*out).imports[i]).write(v) };
    }
    for i in 0..h.export_count as usize {
        let Some(r) =
            (unsafe { record::<r4m::ExportRecord>(reader, &h, i, 3, b"r4m-export-table") })
        else {
            return false;
        };
        let mut name = [0; 128];
        let Some(name) = (unsafe { string(reader, r.name_offset, &mut name, b"r4m-export-name") })
        else {
            return false;
        };
        if unsafe { fmt::r4os_rust_mod_name(span(name), 2) } == 0 || r.version == 0 {
            ctl::log(b"[MOD] malformed export module=");
            ctl::log(default);
            ctl::log(b" symbol=");
            ctl::log(name);
            ctl::log(b"\r\n");
            return false;
        }
        if r.section as usize >= sect.len()
            || sect[r.section as usize].flags & 1 == 0
            || r.offset >= sect[r.section as usize].mem_size
        {
            return false;
        }
        for j in 0..i {
            let prior = unsafe { &(*out).exports[j] };
            if name.eq_ignore_ascii_case(&prior.name[..prior.name_len]) {
                ctl::log(b"[MOD] duplicate export module=");
                ctl::log(default);
                ctl::log(b" symbol=");
                ctl::log(name);
                ctl::log(b"\r\n");
                return false;
            }
        }
        let mut v: query::Export = unsafe { mem::zeroed() };
        v.used = true;
        v.name_len = copy(name, &mut v.name);
        v.version = r.version;
        v.section_index = r.section;
        v.section_offset = r.offset;
        unsafe { ptr::addr_of_mut!((*out).exports[i]).write(v) };
    }
    if h.meta_size <= 2048 {
        let mut data = [0; 2048];
        if let Some(meta) = unsafe { metadata(reader, &h, &mut data, b"r4m-module-name") } {
            unsafe { r4m::r4os_rust_r4m_labels(span(meta), ptr::addr_of_mut!((*out).labels)) };
            if let Some(name) = first_meta(meta) {
                let len = copy(name, unsafe { &mut (*out).name });
                unsafe { ptr::addr_of_mut!((*out).name_len).write(len) };
            }
        }
    }
    true
}

struct Lease<'a> {
    load: state::Prepared,
    peers: &'a state::Peers,
    transferred: bool,
}
impl<'a> Lease<'a> {
    unsafe fn begin(peers: &'a state::Peers) -> Option<Self> {
        let mut out = MaybeUninit::<state::Prepared>::uninit();
        if !unsafe { state::r4os_rust_mod_begin(out.as_mut_ptr(), peers) } {
            return None;
        }
        Some(Self {
            load: unsafe { out.assume_init() },
            peers,
            transferred: false,
        })
    }
    unsafe fn commit(&mut self) -> bool {
        (unsafe { state::r4os_rust_mod_commit(&mut self.load, self.peers) }) != usize::MAX
    }
}
impl Drop for Lease<'_> {
    fn drop(&mut self) {
        if !self.transferred {
            unsafe { state::r4os_rust_mod_abort(&mut self.load, self.peers) };
        }
    }
}
#[derive(Default)]
struct RelocStats {
    abs64: u32,
    rel32: u32,
    base_rel64: u32,
    import_slot64: u32,
}
fn relocation_error(result: u8, kind: u32, module: &[u8]) -> bool {
    match result {
        0 => return true,
        1 => {
            ctl::add(ctl::UNKNOWN);
            ctl::log(b"[MOD] relocation error unknown type=");
            ctl::decimal(u64::from(kind));
            ctl::log(b" module=");
        }
        2 => {
            ctl::add(ctl::RANGE);
            ctl::log(b"[MOD] relocation range error module=");
        }
        3 => {
            ctl::add(ctl::UNRESOLVED);
            ctl::log(b"[MOD] relocation import unresolved module=");
        }
        _ => unreachable!(),
    };
    ctl::log(module);
    ctl::log(b"\r\n");
    false
}
// Keep the original separate image/relocation coordinates explicit during this port.
#[allow(clippy::too_many_arguments)]
unsafe fn relocations(
    reader: *mut Reader,
    data: Option<&[u8]>,
    h: &Header,
    sect: &[Section],
    offsets: &[usize],
    image: *mut u8,
    len: usize,
    module: &[u8],
    imports: &[Resolved],
) -> Option<RelocStats> {
    let image_io = fmt::RelocationImage {
        sections: sect.as_ptr(),
        section_count: sect.len(),
        offsets: offsets.as_ptr(),
        offset_count: offsets.len(),
        image,
        image_len: len,
        imports: imports.as_ptr(),
        import_count: imports.len(),
    };
    let mut stats = RelocStats::default();
    let mut cursor = 0;
    for i in 0..h.reloc_count as usize {
        let mut record = MaybeUninit::<r4m::RelocationRecord>::uninit();
        let ok = if let Some(data) = data {
            unsafe { fmt::r4os_rust_mod_reloc_record(h, span(data), i, record.as_mut_ptr()) }
        } else {
            let mut context = MetaContext {
                reader,
                name: span(b"r4m-relocation-table"),
            };
            let io = unsafe { io(reader, &mut context, b"r4m-relocation-table") };
            unsafe { r4m::r4os_rust_r4m_next(&io, h, &mut cursor, record.as_mut_ptr()) }
        };
        if !ok {
            return None;
        }
        let r = unsafe { record.assume_init() };
        if !relocation_error(
            unsafe { fmt::r4os_rust_mod_reloc(r, &image_io) },
            r.kind,
            module,
        ) {
            return None;
        }
        match r.kind {
            1 => stats.abs64 += 1,
            2 => stats.rel32 += 1,
            3 => stats.base_rel64 += 1,
            4 => stats.import_slot64 += 1,
            _ => {}
        }
    }
    Some(stats)
}
// Keep the original separate image/relocation coordinates explicit during this port.
#[allow(clippy::too_many_arguments)]
unsafe fn loaded_export(
    name: &[u8],
    version: u32,
    index: u32,
    offset: u32,
    sect: &[Section],
    offsets: &[usize],
    image: *mut u8,
    len: usize,
    module: &[u8],
) -> bool {
    let io = fmt::LoadedExport {
        name: span(name),
        version,
        section_index: index,
        export_offset: offset,
        sections: sect.as_ptr(),
        section_count: sect.len(),
        offsets: offsets.as_ptr(),
        offset_count: offsets.len(),
        image: t::Bytes { data: image, len },
    };
    let reason = unsafe { fmt::r4os_rust_mod_loaded_export(&io) };
    reason.len == 0 || unsafe { ctl::reject(span(module), span(name), reason) }
}
struct ResourceContext {
    reader: *mut Reader,
    base: usize,
}
unsafe extern "C" fn resource_read(
    raw: *mut c_void,
    offset: usize,
    out: *mut u8,
    len: usize,
) -> bool {
    let c = unsafe { &*raw.cast::<ResourceContext>() };
    unsafe {
        read(
            c.reader,
            c.base + offset,
            out,
            len,
            b"driver-resources",
            false,
        )
    }
}
// Keep the original separate image/relocation coordinates explicit during this port.
#[allow(clippy::too_many_arguments)]
unsafe fn image(
    load: &state::Prepared,
    data: Option<&[u8]>,
    source: *const file::Source,
    expected: u16,
    default: &[u8],
    path: &[u8],
    imports: &[Resolved],
    peers: &state::Peers,
) -> bool {
    let scratch = unsafe { scratch(load) };
    let table = unsafe { ptr::addr_of_mut!((*scratch).tables) };
    let h = if let Some(data) = data {
        let mut h = MaybeUninit::<Header>::uninit();
        if !unsafe { fmt::r4os_rust_mod_header(span(data), h.as_mut_ptr()) } {
            return report(b"[MOD] bad R4M0 header: ", default);
        }
        let h = unsafe { h.assume_init() };
        if r4m::r4os_rust_r4m_kind(h.kind_raw) == 0 {
            return report(b"[MOD] unknown ModuleKind: ", default);
        }
        if h.kind_raw != expected {
            return report(b"[MOD] unexpected ModuleKind: ", default);
        }
        if !unsafe { fmt::r4os_rust_mod_validate(span(data), &h, 0) } {
            return report(b"[MOD] invalid R4M0 tables: ", default);
        }
        if !unsafe {
            fmt::r4os_rust_mod_sections(
                span(data),
                &h,
                ptr::addr_of_mut!((*table).sections).cast(),
                16,
            )
        } {
            return report(b"[MOD] invalid R4M0 sections: ", default);
        }
        unsafe { ptr::addr_of_mut!((*table).header).write(h) };
        h
    } else {
        if !file::r4os_rust_module_file_allowed(unsafe { (*source).drive }) {
            return false;
        }
        let h = unsafe { (*table).header };
        if r4m::r4os_rust_r4m_kind(h.kind_raw) == 0 {
            return report(b"[MOD] unknown ModuleKind: ", default);
        }
        if h.kind_raw != expected {
            return report(b"[MOD] unexpected ModuleKind: ", default);
        }
        h
    };
    let sect = unsafe { sections(table) };
    let mut resource_file_offset = 0;
    if data.is_none() {
        let catalog = state::catalog(load.slot).cast_mut();
        unsafe { ptr::addr_of_mut!((*catalog).count).write(0) };
        if h.kind_raw == 3 {
            for section in sect {
                if section.flags & 1 != 0 {
                    continue;
                }
                let mut context = ResourceContext {
                    reader: unsafe { ptr::addr_of_mut!((*scratch).reader) },
                    base: section.file_off as usize,
                };
                let io = resources::Reader {
                    context: ptr::from_mut(&mut context).cast(),
                    read: resource_read,
                };
                if unsafe {
                    resources::r4os_rust_module_resource_parse(
                        catalog,
                        &io,
                        section.file_size as usize,
                    )
                } != 0
                {
                    ctl::log(b"[MOD] invalid R4D resource catalog\r\n");
                    return false;
                }
                resource_file_offset = section.file_off as usize;
            }
        }
    }
    let offsets = unsafe { ptr::addr_of_mut!((*scratch).offsets).cast::<usize>() };
    let mut image_size = 0;
    if !unsafe { fmt::r4os_rust_mod_layout(sect.as_ptr(), sect.len(), offsets, &mut image_size) } {
        return report(b"[MOD] R4M0 image layout failed: ", default);
    }
    let image = unsafe { (peers.alloc)(image_size, 4096) };
    if image.is_null() {
        return report(b"[MOD] image allocation failed: ", default);
    }
    unsafe { ptr::write_bytes(image, 0, image_size) };
    let entry = state::entry(load.slot);
    unsafe {
        ptr::addr_of_mut!((*entry).base).write(image as u64);
        ptr::addr_of_mut!((*entry).size).write(image_size)
    };
    let module = if let Some(data) = data {
        if h.meta_size == 0 {
            fallback(default)
        } else {
            fmt::zstring(data, h.meta_off).unwrap_or_else(|| fallback(default))
        }
    } else {
        unsafe { table_name(table, default) }
    };
    let mut runtime: [query::Section; 16] = unsafe { mem::zeroed() };
    let offsets = unsafe { slice::from_raw_parts(offsets, sect.len()) };
    let mut bss = true;
    for (i, s) in sect.iter().enumerate() {
        let off = offsets[i];
        let alloc = s.flags & 1 != 0;
        let size = s.file_size as usize;
        if alloc && size != 0 {
            if let Some(data) = data {
                unsafe {
                    ptr::copy_nonoverlapping(
                        data.as_ptr().add(s.file_off as usize),
                        image.add(off),
                        size,
                    )
                };
            } else if !unsafe {
                file::r4os_rust_module_file_exact(
                    source,
                    s.file_off as usize,
                    image.add(off),
                    size,
                    span(b"r4m-section-image"),
                    true,
                )
            } {
                return false;
            }
        }
        if alloc
            && s.flags & 8 != 0
            && unsafe { slice::from_raw_parts(image.add(off), s.mem_size as usize) }
                .iter()
                .any(|b| *b != 0)
        {
            bss = false;
        }
        runtime[i] = query::Section {
            used: true,
            name: s.name,
            name_len: s.name_len,
            flags: s.flags,
            file_size: s.file_size,
            mem_size: s.mem_size,
            alignment: s.alignment,
            runtime_offset: off,
            runtime_base: if alloc { image as u64 + off as u64 } else { 0 },
        };
    }
    if !bss {
        return false;
    }
    let Some(stats) = (unsafe {
        relocations(
            ptr::addr_of_mut!((*scratch).reader),
            data,
            &h,
            sect,
            offsets,
            image,
            image_size,
            default,
            imports,
        )
    }) else {
        return false;
    };
    if h.kind_raw == 2 {
        for i in 0..h.export_count as usize {
            let (name, version, index, offset) = if let Some(data) = data {
                let off = h.export_off as usize + i * 16;
                let name = fmt::zstring(data, le32(&data[off..])).unwrap();
                (
                    name,
                    le32(&data[off + 12..]),
                    le32(&data[off + 4..]),
                    le32(&data[off + 8..]),
                )
            } else {
                let e = unsafe { &(*table).exports[i] };
                (
                    &e.name[..e.name_len],
                    e.version,
                    e.section_index,
                    e.section_offset,
                )
            };
            if !unsafe {
                loaded_export(
                    name, version, index, offset, sect, offsets, image, image_size, default,
                )
            } {
                return false;
            }
        }
    }
    let mut e = state::Entry {
        kind: h.kind_raw,
        state: 2,
        flags: h.flags,
        base: image as u64,
        size: image_size,
        section_count: h.section_count,
        entry_count: h.entry_count,
        import_count: h.import_count,
        export_count: h.export_count,
        reloc_count: h.reloc_count,
        reloc_abs64_count: stats.abs64,
        reloc_rel32_count: stats.rel32,
        reloc_base_rel64_count: stats.base_rel64,
        reloc_import_slot64_count: stats.import_slot64,
        resolved_import_count: unsafe {
            query::r4os_rust_mod_resolved_count(imports.as_ptr(), imports.len())
        },
        resolver_status: 2,
        bss_zeroed: bss,
        generation: load.generation,
        pinned: h.kind_raw == 2,
        resource_file_offset,
        sections: runtime,
        ..state::Entry::EMPTY
    };
    e.name_len = copy(module, &mut e.name);
    e.path_len = copy(path, &mut e.path);
    if data.is_none() {
        e.version_labels = unsafe { ptr::read(ptr::addr_of!((*table).labels)) };
        if h.kind_raw == 3 {
            e.resource_source.present = true;
            e.resource_source.value = MaybeUninit::new(unsafe { ptr::read(source) });
        }
    }
    for i in 0..(h.export_count as usize).min(16) {
        let (name, version, index, offset) = if let Some(data) = data {
            let off = h.export_off as usize + i * 16;
            let Some(name) = fmt::zstring(data, le32(&data[off..])) else {
                continue;
            };
            (
                name,
                le32(&data[off + 12..]),
                le32(&data[off + 4..]),
                le32(&data[off + 8..]),
            )
        } else {
            let e = unsafe { &(*table).exports[i] };
            (
                &e.name[..e.name_len],
                e.version,
                e.section_index,
                e.section_offset,
            )
        };
        if index >= e.section_count || index >= 16 {
            continue;
        }
        let section = &e.sections[index as usize];
        if !section.used || section.flags & 1 == 0 || offset >= section.mem_size {
            continue;
        }
        e.exports[i].used = true;
        e.exports[i].version = version;
        e.exports[i].address = section.runtime_base + u64::from(offset);
        e.exports[i].section_index = index;
        e.exports[i].section_offset = offset;
        e.exports[i].name_len = copy(name, &mut e.exports[i].name);
    }
    if let Some(data) = data
        && h.meta_size <= 2048
        && fmt::range(data.len(), h.meta_off, h.meta_size)
    {
        unsafe {
            r4m::r4os_rust_r4m_labels(
                span(&data[h.meta_off as usize..][..h.meta_size as usize]),
                &mut e.version_labels,
            )
        };
    }
    unsafe {
        ptr::write(entry, e);
        ctl::r4os_rust_mod_required_logged(entry)
    }
}
fn le32(data: &[u8]) -> u32 {
    u32::from_le_bytes(data[..4].try_into().unwrap())
}

unsafe fn resolved(load: &state::Prepared, count: u32) -> &[Resolved] {
    unsafe {
        slice::from_raw_parts(
            ptr::addr_of!((*scratch(load)).resolved).cast(),
            count as usize,
        )
    }
}
unsafe fn resolve_one(
    load: &state::Prepared,
    index: usize,
    request: fmt::Import,
    importer: &[u8],
) -> bool {
    let out = unsafe { ptr::addr_of_mut!((*scratch(load)).resolved[index]).cast::<Resolved>() };
    let Some(slot) = (unsafe { ctl::find_name(bytes(request.module)) }) else {
        if request.flags & 1 != 0 {
            unsafe { query::r4os_rust_mod_optional(request.module, request.symbol, out) };
            return true;
        }
        ctl::add(ctl::MISSING);
        ctl::log(b"[MOD] resolver missing module=");
        ctl::log(unsafe { bytes(request.module) });
        ctl::log(b" importer=");
        ctl::log(importer);
        ctl::log(b"\r\n");
        return false;
    };
    unsafe {
        ctl::r4os_rust_mod_resolve_logged(
            state::entry(slot),
            request.module,
            request.symbol,
            request.min_version,
            request.flags,
            span(importer),
            out,
        )
    }
}
/// # Safety
/// Return the fixed private native Scratch layout for this exact implementation.
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_mod_scratch_layout(align: bool) -> usize {
    if align {
        mem::align_of::<Scratch>()
    } else {
        mem::size_of::<Scratch>()
    }
}
/// # Safety
/// Source/data/names remain resident through all admitted I/O. Peers use this
/// exact native Scratch layout; output is aligned writable Prepared storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_prepare(
    data: t::Bytes,
    source: *const file::Source,
    expected: u16,
    default: t::Bytes,
    path: t::Bytes,
    peers: &state::Peers,
    out: *mut state::Prepared,
) -> bool {
    let Some(mut lease) = (unsafe { Lease::begin(peers) }) else {
        return false;
    };
    let default = unsafe { bytes(default) };
    let path = unsafe { bytes(path) };
    let (h, importer) = if source.is_null() {
        let data = unsafe { bytes(data) };
        let mut h = MaybeUninit::<Header>::uninit();
        if !unsafe { fmt::r4os_rust_mod_header(span(data), h.as_mut_ptr()) } {
            return report(b"[MOD] bad R4M0 header: ", default);
        }
        let h = unsafe { h.assume_init() };
        if r4m::r4os_rust_r4m_kind(h.kind_raw) == 0 {
            return report(b"[MOD] unknown ModuleKind: ", default);
        }
        if !r4m::r4os_rust_r4m_loadable(h.kind_raw) || !r4m::r4os_rust_r4m_loadable(expected) {
            return report(b"[MOD] reserved ModuleKind: ", default);
        }
        if h.kind_raw != expected {
            return report(b"[MOD] unexpected ModuleKind: ", default);
        }
        if !unsafe { fmt::r4os_rust_mod_validate(span(data), &h, 0) } {
            return report(b"[MOD] invalid R4M0 tables: ", default);
        }
        (
            h,
            if h.meta_size == 0 {
                fallback(default)
            } else {
                fmt::zstring(data, h.meta_off).unwrap_or_else(|| fallback(default))
            },
        )
    } else {
        if !r4m::r4os_rust_r4m_loadable(expected) {
            return report(b"[MOD] reserved ModuleKind: ", default);
        }
        let scratch = unsafe { scratch(&lease.load) };
        unsafe { reader_init(ptr::addr_of_mut!((*scratch).reader), source) };
        let table = unsafe { ptr::addr_of_mut!((*scratch).tables) };
        if !unsafe {
            tables(
                ptr::addr_of_mut!((*scratch).reader),
                table,
                expected,
                default,
            )
        } {
            return report(b"[MOD] bad R4M0 header: ", default);
        }
        let h = unsafe { (*table).header };
        if r4m::r4os_rust_r4m_kind(h.kind_raw) == 0 {
            return report(b"[MOD] unknown ModuleKind: ", default);
        }
        if h.kind_raw != expected {
            return report(b"[MOD] unexpected ModuleKind: ", default);
        }
        (h, unsafe { table_name(table, default) })
    };
    if h.import_count as usize > N {
        ctl::add(ctl::UNRESOLVED);
        return report(b"[MOD] too many imports importer=", importer);
    }
    for i in 0..h.import_count as usize {
        let request = if source.is_null() {
            let mut request = MaybeUninit::<fmt::Import>::uninit();
            if !unsafe { fmt::r4os_rust_mod_import(&h, data, i, request.as_mut_ptr()) } {
                ctl::add(ctl::UNRESOLVED);
                return report(b"[MOD] bad import importer=", importer);
            }
            unsafe { request.assume_init() }
        } else {
            unsafe { (*scratch(&lease.load)).tables.imports[i].request() }
        };
        if !unsafe { resolve_one(&lease.load, i, request, importer) } {
            return false;
        }
    }
    if !unsafe {
        image(
            &lease.load,
            if source.is_null() {
                Some(bytes(data))
            } else {
                None
            },
            source,
            expected,
            default,
            path,
            resolved(&lease.load, h.import_count),
            peers,
        )
    } {
        return false;
    }
    unsafe { ptr::write(out, ptr::read(&lease.load)) };
    lease.transferred = true;
    true
}

struct Pending {
    used: bool,
    state: u8,
    source: MaybeUninit<file::Source>,
    file_size: usize,
    file_name: [u8; 32],
    file_name_len: usize,
    name: [u8; 32],
    name_len: usize,
    path: [u8; 96],
    path_len: usize,
}
impl Pending {
    const EMPTY: Self = Self {
        used: false,
        state: 0,
        source: MaybeUninit::uninit(),
        file_size: 0,
        file_name: [0; 32],
        file_name_len: 0,
        name: [0; 32],
        name_len: 0,
        path: [0; 96],
        path_len: 0,
    };
}
unsafe fn pending_name<'a>(p: *const Pending) -> &'a [u8] {
    unsafe { slice::from_raw_parts(ptr::addr_of!((*p).name).cast(), (*p).name_len) }
}
unsafe fn pending_file_name<'a>(p: *const Pending) -> &'a [u8] {
    unsafe { slice::from_raw_parts(ptr::addr_of!((*p).file_name).cast(), (*p).file_name_len) }
}
unsafe fn pending_path<'a>(p: *const Pending) -> &'a [u8] {
    unsafe { slice::from_raw_parts(ptr::addr_of!((*p).path).cast(), (*p).path_len) }
}
// The two-window metadata reader belongs only to this resident probe workspace.
#[inline(never)]
unsafe fn probe(source: *const file::Source, file_name: &[u8], out: *mut Pending) -> bool {
    let Some(reader) = ResidentScratch::<Reader>::new() else {
        return report(b"[MOD] metadata reader allocation failed: ", file_name);
    };
    unsafe { reader_init(reader.as_mut_ptr(), source) };
    let Some(h) = (unsafe { header(reader.as_mut_ptr(), 2, b"r4l-metadata-probe") }) else {
        return false;
    };
    let mut data = [0; 2048];
    let Some(meta) =
        (unsafe { metadata(reader.as_mut_ptr(), &h, &mut data, b"r4l-metadata-probe") })
    else {
        return false;
    };
    let name = first_meta(meta).unwrap_or_else(|| fallback(file_name));
    if unsafe { fmt::r4os_rust_mod_name(span(name), 0) } == 0 {
        return report(b"[MOD] invalid R4L module name file=", file_name);
    }
    if unsafe { ctl::find_name(name) }.is_some() {
        ctl::add(ctl::DUPLICATE);
        return report(b"[MOD] resolver duplicate provider module=", name);
    }
    unsafe {
        ptr::write(out, Pending::EMPTY);
        ptr::copy_nonoverlapping(source, ptr::addr_of_mut!((*out).source).cast(), 1);
        (*out).file_size = (*source).entry.size as usize;
        (*out).file_name_len = copy(file_name, &mut (*out).file_name);
        (*out).name_len = copy(name, &mut (*out).name);
        (*out).path_len = copy(b"C:\\R4OS\\LIBS\\", &mut (*out).path);
        let prefix = (*out).path_len;
        (*out).path_len += copy(file_name, &mut (&mut (*out).path)[prefix..]);
        (*out).used = true;
        (*out).state = 1;
    }
    true
}
unsafe fn duplicates(pending: *mut Pending, count: usize) {
    for i in 0..count {
        let p = unsafe { pending.add(i) };
        if unsafe { !(*p).used || (*p).state == 4 } {
            continue;
        }
        for j in i + 1..count {
            let q = unsafe { pending.add(j) };
            if unsafe { !(*q).used || (*q).state == 4 } {
                continue;
            }
            if unsafe { pending_name(p) }.eq_ignore_ascii_case(unsafe { pending_name(q) }) {
                ctl::add(ctl::DUPLICATE);
                unsafe {
                    (*p).state = 4;
                    (*q).state = 4;
                };
                report(b"[MOD] resolver duplicate provider module=", unsafe {
                    pending_name(p)
                });
            }
        }
    }
}
unsafe fn pending_by_name(pending: *mut Pending, count: usize, name: &[u8]) -> Option<usize> {
    for i in 0..count {
        let p = unsafe { pending.add(i) };
        if unsafe { (*p).used } && unsafe { pending_name(p) }.eq_ignore_ascii_case(name) {
            return Some(i);
        }
    }
    None
}
unsafe fn resolve_pending_import(
    pending: *mut Pending,
    count: usize,
    importer: usize,
    request: fmt::Import,
    peers: &state::Peers,
    out: *mut Resolved,
) -> bool {
    let name = unsafe { pending_name(pending.add(importer)) };
    let module = unsafe { bytes(request.module) };
    if let Some(slot) = unsafe { ctl::find_name(module) } {
        return unsafe {
            ctl::r4os_rust_mod_resolve_logged(
                state::entry(slot),
                request.module,
                request.symbol,
                request.min_version,
                request.flags,
                span(name),
                out,
            )
        };
    }
    let Some(provider) = (unsafe { pending_by_name(pending, count, module) }) else {
        if request.flags & 1 != 0 {
            unsafe { query::r4os_rust_mod_optional(request.module, request.symbol, out) };
            return true;
        }
        ctl::add(ctl::MISSING);
        ctl::log(b"[MOD] resolver missing module=");
        ctl::log(module);
        ctl::log(b" importer=");
        ctl::log(name);
        ctl::log(b"\r\n");
        return false;
    };
    if unsafe { (*pending.add(provider)).state } == 2 {
        if request.flags & 1 != 0 {
            unsafe { query::r4os_rust_mod_optional(request.module, request.symbol, out) };
            return true;
        }
        ctl::add(ctl::CYCLE);
        unsafe {
            (*pending.add(provider)).state = 4;
        };
        ctl::log(b"[MOD] resolver cycle importer=");
        ctl::log(name);
        ctl::log(b" provider=");
        ctl::log(module);
        ctl::log(b"\r\n");
        return false;
    }
    if !unsafe { resolve_pending(pending, count, provider, peers) } {
        if request.flags & 1 != 0 {
            unsafe { query::r4os_rust_mod_optional(request.module, request.symbol, out) };
            return true;
        }
        return false;
    }
    if let Some(slot) = unsafe { ctl::find_name(module) } {
        return unsafe {
            ctl::r4os_rust_mod_resolve_logged(
                state::entry(slot),
                request.module,
                request.symbol,
                request.min_version,
                request.flags,
                span(name),
                out,
            )
        };
    }
    ctl::add(ctl::UNRESOLVED);
    false
}
unsafe fn resolve_pending(
    pending: *mut Pending,
    count: usize,
    index: usize,
    peers: &state::Peers,
) -> bool {
    if index >= count || unsafe { !(*pending.add(index)).used } {
        return false;
    }
    let p = unsafe { pending.add(index) };
    match unsafe { (*p).state } {
        3 => return true,
        0 | 4 => return false,
        2 => {
            ctl::add(ctl::CYCLE);
            unsafe {
                (*p).state = 4;
            };
            return report(b"[MOD] resolver cycle at ", unsafe { pending_name(p) });
        }
        1 => {}
        _ => unreachable!(),
    }
    unsafe {
        (*p).state = 2;
    };
    let Some(mut lease) = (unsafe { Lease::begin(peers) }) else {
        unsafe {
            (*p).state = 4;
        };
        return false;
    };
    let scratch = unsafe { scratch(&lease.load) };
    let source = unsafe { ptr::addr_of!((*p).source).cast::<file::Source>() };
    unsafe {
        reader_init(ptr::addr_of_mut!((*scratch).reader), source);
        (*scratch).reader.file_size = (*p).file_size;
    };
    let table = unsafe { ptr::addr_of_mut!((*scratch).tables) };
    if !unsafe {
        tables(
            ptr::addr_of_mut!((*scratch).reader),
            table,
            2,
            pending_file_name(p),
        )
    } {
        unsafe {
            (*p).state = 4;
        };
        return false;
    }
    let h = unsafe { (*table).header };
    if h.import_count as usize > N {
        unsafe {
            (*p).state = 4;
        };
        ctl::add(ctl::UNRESOLVED);
        return false;
    }
    for i in 0..h.import_count as usize {
        let request = unsafe { (&(*table).imports)[i].request() };
        let out = unsafe { ptr::addr_of_mut!((*scratch).resolved[i]).cast() };
        if !unsafe { resolve_pending_import(pending, count, index, request, peers, out) } {
            if unsafe { (*p).state } == 2 {
                unsafe {
                    (*p).state = 4;
                };
            }
            return false;
        }
    }
    if unsafe { (*p).state } != 2 {
        return false;
    }
    let start = super::performance::now();
    let success = unsafe {
        image(
            &lease.load,
            None,
            source,
            2,
            pending_file_name(p),
            pending_path(p),
            resolved(&lease.load, h.import_count),
            peers,
        )
    } && unsafe { lease.commit() };
    super::performance::add_r4l_load_ticks(start);
    unsafe {
        (*p).state = if success { 3 } else { 4 };
    };
    success
}
/// # Safety
/// Original single boot-generation library admission. Peers remain immutable
/// and provide the exact native Scratch/heap contract throughout all I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_load_libraries(peers: &state::Peers) {
    if ctl::r4os_rust_mod_libraries_active(false) {
        ctl::log(b"[MOD] active R4L generation is pinned; reload deferred until reboot\r\n");
        return;
    }
    let mut volume = MaybeUninit::<t::Volume>::uninit();
    if !unsafe {
        vfs::r4os_rust_vfs_mount(
            4,
            b'C',
            ptr::null(),
            ptr::null(),
            volume.as_mut_ptr().cast(),
        )
    } {
        ctl::log(b"[MOD] C: FAT32 volume not mounted\r\n");
        return;
    }
    let volume = unsafe { volume.assume_init() };
    let result = unsafe {
        vfs::r4os_rust_vfs_run(
            0,
            &t::Args {
                volume,
                name: span(b"/R4OS/LIBS"),
                ..t::Args::default()
            },
            ptr::null_mut(),
        )
    };
    if result.status != 0 {
        ctl::log(b"[MOD] /R4OS/LIBS not found\r\n");
        return;
    }
    let dir = result.value;
    // This scan owns the fixed 32-record set through dependency resolution.
    // Keep it off the bounded boot/task stack and initialize only in place.
    let Some(storage) = ResidentScratch::<[Pending; 32]>::new() else {
        ctl::log(b"[MOD] pending library workspace allocation failed\r\n");
        return;
    };
    ctl::r4os_rust_mod_libraries_active(true);
    ctl::log(b"[MOD] scanning /R4OS/LIBS\r\n");
    let pending = storage.as_mut_ptr().cast::<Pending>();
    for i in 0..32 {
        unsafe { ptr::write(pending.add(i), Pending::EMPTY) };
    }
    let mut count = 0;
    let mut index = 0;
    let mut name = [0; 64];
    loop {
        let start = super::performance::now();
        let mut entry = MaybeUninit::<t::Entry>::uninit();
        let result = unsafe {
            vfs::r4os_rust_vfs_run(
                26,
                &t::Args {
                    volume,
                    parent: dir,
                    index,
                    buffer: name.as_mut_ptr(),
                    buffer_len: name.len(),
                    ..t::Args::default()
                },
                entry.as_mut_ptr(),
            )
        };
        super::performance::add_r4l_scan_ticks(start);
        if result.status != 0 {
            break;
        }
        super::performance::record_r4l_scan_entry();
        let entry = unsafe { entry.assume_init() };
        let file_name = &name[..name_len(&name)];
        if !vfs::r4os_rust_vfs_attribute(0, entry.attr)
            && unsafe { fmt::r4os_rust_mod_name(span(file_name), 3) } != 0
        {
            super::performance::record_r4l_candidate();
            if count >= 32 {
                ctl::log(b"[MOD] pending library limit reached\r\n");
                break;
            }
            if entry.size != 0
                && unsafe {
                    probe(
                        &file::Source {
                            volume,
                            entry,
                            drive: b'C',
                        },
                        file_name,
                        pending.add(count),
                    )
                }
            {
                count += 1;
            }
        }
        index += 1;
    }
    unsafe { duplicates(pending, count) };
    let mut loaded = 0;
    let start = super::performance::now();
    for i in 0..count {
        if unsafe { resolve_pending(pending, count, i, peers) } {
            loaded += 1;
        }
    }
    super::performance::add_r4l_resolve_ticks(start);
    super::performance::record_r4l_results(loaded, count);
    ctl::r4os_rust_mod_library_results(loaded);
    // Original freePendingLibraries only clears its used records; no retained
    // allocation or payload is owned by this temporary metadata array.
    for i in 0..count {
        if unsafe { (*pending.add(i)).used } {
            unsafe { ptr::write(pending.add(i), Pending::EMPTY) };
        }
    }
}

/// # Safety
/// Same resident admission inputs and native peer contract as `prepare`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_load(
    data: t::Bytes,
    source: *const file::Source,
    expected: u16,
    default: t::Bytes,
    path: t::Bytes,
    peers: &state::Peers,
) -> usize {
    let mut out = MaybeUninit::<state::Prepared>::uninit();
    if !unsafe {
        r4os_rust_mod_prepare(
            data,
            source,
            expected,
            default,
            path,
            peers,
            out.as_mut_ptr(),
        )
    } {
        return usize::MAX;
    }
    let mut load = unsafe { out.assume_init() };
    let result = unsafe { state::r4os_rust_mod_commit(&mut load, peers) };
    unsafe { state::r4os_rust_mod_abort(&mut load, peers) };
    result
}

/// # Safety
/// Handle belongs to its original caller; symbol and output storage are resident.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_prepared_export(
    load: *const state::Prepared,
    symbol: t::Bytes,
    min_version: u32,
    out: *mut u64,
) -> bool {
    if !unsafe { state::r4os_rust_mod_owns(load) } {
        return false;
    }
    let view = unsafe { ctl::view(state::entry((*load).slot)) };
    let export = unsafe { query::r4os_rust_mod_find_export(&view, symbol) };
    if export.is_null() || unsafe { (*export).version } < min_version {
        return false;
    }
    unsafe { out.write((*export).address) };
    true
}
