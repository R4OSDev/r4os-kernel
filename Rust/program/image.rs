//! R4X image preparation: original sections/exports/imports/relocations and
//! explicit image rollback. The caller retains its exact bounded file reader.
use super::{memory, policy};
use crate::{
    contract,
    fs::vfs_types::Bytes,
    loader::{r4m, registry_control as modules},
};
use core::{ffi::c_void, mem::MaybeUninit, ptr, slice};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Image {
    pub range_id: u32,
    pub code: *mut u8,
    pub len: usize,
    pub owner_id: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ImportSeed {
    pub group_id: u32,
    pub min_version: u32,
    pub resolved_version: u32,
    pub flags: u32,
    pub table: u64,
    pub r4l_binding_valid: bool,
    pub r4l_module_slot: u8,
    pub r4l_generation: u32,
    pub module_name: [u8; 32],
    pub module_name_len: usize,
    pub symbol_name: [u8; 32],
    pub symbol_name_len: usize,
}
impl ImportSeed {
    const EMPTY: Self = Self {
        group_id: 0,
        min_version: 0,
        resolved_version: 0,
        flags: 0,
        table: 0,
        r4l_binding_valid: false,
        r4l_module_slot: 0,
        r4l_generation: 0,
        module_name: [0; 32],
        module_name_len: 0,
        symbol_name: [0; 32],
        symbol_name_len: 0,
    };
}
#[repr(C)]
pub struct Loaded {
    pub image: Image,
    pub entry: u64,
    pub memory: memory::Contract,
    pub parallel_execution: bool,
    pub parallel_workers: bool,
    pub desktop_host: bool,
    pub imports: [ImportSeed; 16],
    pub import_count: u32,
    pub section_count: u32,
    pub relocation_count: u32,
}
#[repr(C)]
pub struct Reader {
    pub context: *mut c_void,
    pub read: unsafe extern "C" fn(*mut c_void, usize, *mut u8, usize, Bytes, bool) -> bool,
}
#[repr(C)]
pub struct Request {
    pub reader: *mut c_void,
    pub source: *const c_void,
    pub header: r4m::Header,
    pub file_size: usize,
    pub owner_id: u32,
    pub app_class: u8,
}
#[repr(C)]
struct ReadContext {
    reader: *mut c_void,
    name: Bytes,
    verbose: bool,
}
#[derive(Clone, Copy)]
struct Section {
    flags: u32,
    file_off: u32,
    file_size: u32,
    mem_size: u32,
    alignment: u32,
}
impl Section {
    const EMPTY: Self = Self {
        flags: 0,
        file_off: 0,
        file_size: 0,
        mem_size: 0,
        alignment: 0,
    };
    fn loadable(self) -> bool {
        self.flags & 1 != 0
    }
}
#[derive(Clone, Copy)]
struct Resolved {
    address: u64,
}

fn span(v: &[u8]) -> Bytes {
    Bytes {
        data: v.as_ptr(),
        len: v.len(),
    }
}
fn log(v: &[u8]) {
    modules::log(v)
}
unsafe extern "C" fn read(context: *mut c_void, offset: usize, out: *mut u8, len: usize) -> bool {
    let c = unsafe { &*context.cast::<ReadContext>() };
    unsafe {
        let reader = &*c.reader.cast::<Reader>();
        (reader.read)(reader.context, offset, out, len, c.name, c.verbose)
    }
}
unsafe extern "C" fn error(context: *mut c_void, reason: *const u8, len: usize) {
    let c = unsafe { &*context.cast::<ReadContext>() };
    if !c.verbose {
        return;
    }
    log(b"[R4M] ");
    log(unsafe { slice::from_raw_parts(reason, len) });
    log(b" failed ");
    log(unsafe { slice::from_raw_parts(c.name.data, c.name.len) });
    log(b"\r\n");
}
fn io(c: &mut ReadContext, size: usize) -> r4m::Io {
    r4m::Io {
        context: ptr::from_mut(c).cast(),
        read,
        log: error,
        file_size: size,
    }
}
fn record<T>(r: &Request, index: usize, op: u8, name: &[u8]) -> Option<T> {
    let mut c = ReadContext {
        reader: r.reader,
        name: span(name),
        verbose: true,
    };
    let io = io(&mut c, r.file_size);
    let mut out = MaybeUninit::<T>::uninit();
    if !unsafe { r4m::r4os_rust_r4m_record(&io, &r.header, index, op, out.as_mut_ptr().cast()) } {
        return None;
    }
    Some(unsafe { out.assume_init() })
}
fn string<'a>(r: &Request, offset: u32, out: &'a mut [u8], name: &[u8]) -> Option<&'a [u8]> {
    let mut c = ReadContext {
        reader: r.reader,
        name: span(name),
        verbose: true,
    };
    let io = io(&mut c, r.file_size);
    let s = unsafe { r4m::r4os_rust_r4m_zstring(&io, offset, out.as_mut_ptr(), out.len()) };
    s.present.then(|| &out[s.offset..s.offset + s.len])
}
fn sections(r: &Request, out: &mut [Section]) -> bool {
    if r.header.section_count as usize > out.len() {
        return false;
    }
    let mut non_alloc_seen = false;
    for (i, out) in out[..r.header.section_count as usize]
        .iter_mut()
        .enumerate()
    {
        let Some(v) = record::<r4m::SectionRecord>(r, i, 0, b"r4x-section-table") else {
            return false;
        };
        let section = Section {
            flags: v.flags,
            file_off: v.file_off,
            file_size: v.file_size,
            mem_size: v.mem_size,
            alignment: v.alignment,
        };
        if section.mem_size == 0
            || section.mem_size < section.file_size
            || !section.alignment.is_power_of_two()
        {
            return false;
        }
        let off = section.file_off as usize;
        let len = section.file_size as usize;
        if len != 0 && (off > r.file_size || len > r.file_size - off) {
            return false;
        }
        if !section.loadable() {
            let n = v.name.iter().position(|v| *v == 0).unwrap_or(v.name.len());
            if section.flags != 0 || non_alloc_seen || &v.name[..n] != b".rsrc" {
                log(b"Invalid R4M0 non-alloc section\r\n");
                return false;
            }
            non_alloc_seen = true;
        }
        *out = section;
    }
    true
}
fn exports(r: &Request, sections: &[Section]) -> Option<(bool, bool)> {
    let mut v1 = false;
    let mut v2 = false;
    for i in 0..r.header.export_count as usize {
        let v = record::<r4m::ExportRecord>(r, i, 3, b"r4x-export-table")?;
        let mut buf = [0u8; 128];
        let name = string(r, v.name_offset, &mut buf, b"r4x-export-name")?;
        if name == b"R4XStart" {
            v1 |= v.version >= 1;
            v2 |= v.version >= 2;
        }
        let section = sections.get(v.section as usize)?;
        if !section.loadable() || v.offset >= section.mem_size {
            return None;
        }
    }
    Some((v1, v2))
}
fn metadata(r: &Request, v1: bool, v2: bool) -> Option<(memory::Contract, bool, bool, bool)> {
    let mut c = ReadContext {
        reader: r.reader,
        name: span(b"r4x-metadata-probe"),
        verbose: true,
    };
    let io = io(&mut c, r.file_size);
    let mut buf = [0u8; 2048];
    let s = unsafe { r4m::r4os_rust_r4m_metadata(&io, &r.header, buf.as_mut_ptr(), buf.len()) };
    if !s.present {
        return None;
    }
    let meta = &buf[s.offset..s.offset + s.len];
    if !unsafe {
        policy::r4os_rust_program_start_accepts(
            meta.as_ptr(),
            meta.len(),
            v1 && !v2,
            r.header.export_count,
        )
    } {
        return None;
    }
    let memory = memory::resolve(meta, r.app_class)?;
    let parallel = unsafe { policy::r4os_rust_program_parallel(meta.as_ptr(), meta.len()) };
    if parallel == u8::MAX {
        return None;
    }
    let workers = policy::parallel_owner(meta, b"runtime.workers=");
    if workers == u8::MAX {
        return None;
    }
    let desktop = unsafe { policy::r4os_rust_program_desktop_host(meta.as_ptr(), meta.len()) };
    if desktop == u8::MAX {
        return None;
    }
    Some((
        memory,
        parallel == 1,
        workers == 1,
        desktop == 1 && r.app_class == 1,
    ))
}
fn align(value: usize, alignment: usize) -> Option<usize> {
    Some(value.checked_add(alignment - 1)? & !(alignment - 1))
}
fn layout(sections: &[Section], offsets: &mut [usize]) -> Option<usize> {
    let mut cursor = 0usize;
    for (i, section) in sections.iter().enumerate() {
        if !section.loadable() {
            offsets[i] = 0;
            continue;
        }
        cursor = align(cursor, (section.alignment as usize).max(4096))?;
        offsets[i] = cursor;
        cursor = cursor.checked_add(section.mem_size as usize)?;
    }
    let size = align(cursor, 4096)?;
    if size == 0 { None } else { Some(size) }
}
fn payload(r: &Request, sections: &[Section], offsets: &[usize], image: Image) -> bool {
    for (i, section) in sections.iter().enumerate() {
        if !section.loadable() || section.file_size == 0 {
            continue;
        }
        if section.flags & 8 != 0 {
            log(b"Invalid R4M0 BSS payload\r\n");
            return false;
        }
        let size = section.file_size as usize;
        let off = offsets[i];
        if off > image.len || size > image.len - off {
            return false;
        }
        if !unsafe {
            program_image_read(
                r.source,
                section.file_off as usize,
                image.code.add(off),
                size,
            )
        } {
            log(b"R4M0 section payload read failed\r\n");
            return false;
        }
    }
    true
}
fn copy_name(src: &[u8], out: &mut [u8; 32], len: &mut usize) -> bool {
    if src.len() >= out.len() {
        return false;
    }
    out.fill(0);
    out[..src.len()].copy_from_slice(src);
    *len = src.len();
    true
}
fn imports(r: &Request, resolved: &mut [Resolved], seeds: &mut [ImportSeed]) -> bool {
    if r.header.import_count as usize > resolved.len()
        || r.header.import_count as usize > seeds.len()
    {
        return false;
    }
    for i in 0..r.header.import_count as usize {
        let mut module_buf = [0u8; 128];
        let mut symbol_buf = [0u8; 128];
        let Some(v) = record::<r4m::ImportRecord>(r, i, 2, b"r4x-import-table") else {
            return false;
        };
        let Some(module) = string(r, v.module_offset, &mut module_buf, b"r4x-import-module") else {
            return false;
        };
        let Some(symbol) = string(r, v.symbol_offset, &mut symbol_buf, b"r4x-import-symbol") else {
            return false;
        };
        if !unsafe { r4m::r4os_rust_r4m_import(span(module), v.flags) } || v.min_version == 0 {
            return false;
        }
        let mut result = MaybeUninit::<modules::ExportInfo>::uninit();
        if !unsafe {
            modules::r4os_rust_mod_resolve_info(
                span(module),
                span(symbol),
                v.min_version,
                result.as_mut_ptr(),
            )
        } {
            if v.flags & 1 != 0 {
                let mut seed = ImportSeed {
                    min_version: v.min_version,
                    ..ImportSeed::EMPTY
                };
                if !copy_name(module, &mut seed.module_name, &mut seed.module_name_len)
                    || !copy_name(symbol, &mut seed.symbol_name, &mut seed.symbol_name_len)
                {
                    return false;
                }
                resolved[i] = Resolved { address: 0 };
                seeds[i] = seed;
                continue;
            }
            log(b"Unresolved R4M0 import ");
            log(module);
            log(b":");
            log(symbol);
            log(b"\r\n");
            return false;
        }
        let result = unsafe { result.assume_init() };
        if v.flags & 1 != 0 && result.kind != 2 {
            return false;
        }
        resolved[i] = Resolved {
            address: result.address,
        };
        let platform = if symbol == b"Query" {
            unsafe { modules::r4os_rust_mod_platform_group(span(module)) }
        } else {
            u64::MAX
        };
        let group = if platform == u64::MAX {
            0
        } else {
            u32::try_from(platform).unwrap()
        };
        let mut seed = ImportSeed {
            group_id: group,
            min_version: v.min_version,
            resolved_version: result.version,
            table: result.address,
            r4l_binding_valid: result.kind == 2,
            r4l_module_slot: result.module_slot,
            r4l_generation: result.generation,
            ..ImportSeed::EMPTY
        };
        if !copy_name(module, &mut seed.module_name, &mut seed.module_name_len)
            || !copy_name(symbol, &mut seed.symbol_name, &mut seed.symbol_name_len)
        {
            return false;
        }
        if symbol == b"Query" {
            let table = super::runtime_tables::interface(group) as u64;
            if table != 0 {
                seed.flags |= contract::r4xstart_import_flag_group_interface;
                seed.table = table;
            }
        }
        if group == 0 && symbol != b"Query" {
            let mut record = crate::boot::record::Record::new();
            record.text(b"[R4X] named import module=");
            record.text(module);
            record.text(b" symbol=");
            record.text(symbol);
            record.text(b" need=");
            record.decimal(u64::from(v.min_version));
            record.text(b" have=");
            record.decimal(u64::from(result.version));
            record.text(b" generation=");
            record.decimal(u64::from(result.generation));
            record.text(b"\r\n");
            log(record.as_bytes());
        }
        seeds[i] = seed;
    }
    true
}
fn patch(
    reloc: r4m::RelocationRecord,
    sections: &[Section],
    offsets: &[usize],
    image: Image,
    size: usize,
) -> Option<usize> {
    let section = sections.get(reloc.patch_section as usize)?;
    if !section.loadable() {
        return None;
    }
    let off = reloc.patch_offset as usize;
    let mem = section.mem_size as usize;
    if off > mem || size > mem - off {
        return None;
    }
    let off = offsets[reloc.patch_section as usize] + off;
    if off > image.len || size > image.len - off {
        return None;
    }
    Some(off)
}
fn target(
    reloc: r4m::RelocationRecord,
    sections: &[Section],
    offsets: &[usize],
    image: Image,
) -> Option<u64> {
    let section = sections.get(reloc.target_section as usize)?;
    if !section.loadable() || reloc.target_offset >= section.mem_size {
        return None;
    }
    Some(
        image.code.addr() as u64
            + offsets[reloc.target_section as usize] as u64
            + u64::from(reloc.target_offset),
    )
}
fn add(base: u64, addend: i32) -> Option<u64> {
    if addend >= 0 {
        base.checked_add(addend as u64)
    } else {
        base.checked_sub((-i64::from(addend)) as u64)
    }
}
fn relocate(
    reloc: r4m::RelocationRecord,
    sections: &[Section],
    offsets: &[usize],
    image: Image,
    imports: &[Resolved],
) -> Option<()> {
    match reloc.kind {
        1 | 3 => {
            let off = patch(reloc, sections, offsets, image, 8)?;
            let value = add(target(reloc, sections, offsets, image)?, reloc.addend)?;
            unsafe {
                ptr::copy_nonoverlapping(value.to_le_bytes().as_ptr(), image.code.add(off), 8)
            };
        }
        2 => {
            let off = patch(reloc, sections, offsets, image, 4)?;
            let value = add(target(reloc, sections, offsets, image)?, reloc.addend)?;
            let patch_address = unsafe { image.code.add(off) }.addr() as u64;
            let delta = i128::from(value) - i128::from(patch_address + 4);
            let delta = i32::try_from(delta).ok()?;
            unsafe {
                ptr::copy_nonoverlapping(delta.to_le_bytes().as_ptr(), image.code.add(off), 4)
            };
        }
        4 => {
            let off = patch(reloc, sections, offsets, image, 8)?;
            let import = imports.get(reloc.target_section as usize)?;
            if import.address == 0 && reloc.addend != 0 {
                return None;
            }
            let value = add(import.address, reloc.addend)?;
            unsafe {
                ptr::copy_nonoverlapping(value.to_le_bytes().as_ptr(), image.code.add(off), 8)
            };
        }
        _ => {
            log(b"Unsupported R4M0 relocation\r\n");
            return None;
        }
    }
    Some(())
}
fn relocations(
    r: &Request,
    sections: &[Section],
    offsets: &[usize],
    image: Image,
    imports: &[Resolved],
) -> Option<()> {
    let mut c = ReadContext {
        reader: r.reader,
        name: span(b"r4x-relocation-table"),
        verbose: true,
    };
    let io = io(&mut c, r.file_size);
    let mut index = 0;
    for _ in 0..r.header.reloc_count {
        let mut v = MaybeUninit::uninit();
        if !unsafe { r4m::r4os_rust_r4m_next(&io, &r.header, &mut index, v.as_mut_ptr()) } {
            return None;
        }
        relocate(
            unsafe { v.assume_init() },
            sections,
            offsets,
            image,
            imports,
        )?;
    }
    Some(())
}
#[allow(clippy::too_many_arguments)]
fn finish(
    r: &Request,
    sections: &[Section],
    offsets: &[usize],
    image: Image,
    memory: memory::Contract,
    parallel: bool,
    workers: bool,
    desktop: bool,
) -> Option<Loaded> {
    if !payload(r, sections, offsets, image) {
        return None;
    }
    let mut resolved = [Resolved { address: 0 }; 16];
    let mut seeds = [ImportSeed::EMPTY; 16];
    if !imports(r, &mut resolved, &mut seeds) {
        return None;
    }
    relocations(
        r,
        sections,
        offsets,
        image,
        &resolved[..r.header.import_count as usize],
    )?;
    let Some(entry) = record::<r4m::EntryRecord>(r, 0, 1, b"r4x-entry-table") else {
        log(b"Invalid R4M0 entry\r\n");
        return None;
    };
    if entry.kind != 1 {
        log(b"Unsupported R4M0 entry kind\r\n");
        return None;
    }
    let section = entry.section as usize;
    if section >= sections.len() || !sections[section].loadable() {
        log(b"Invalid R4M0 entry section\r\n");
        return None;
    }
    let offset = offsets[section] + entry.offset as usize;
    if offset >= image.len {
        log(b"Invalid R4M0 entry offset\r\n");
        return None;
    }
    Some(Loaded {
        image,
        entry: unsafe { image.code.add(offset) }.addr() as u64,
        memory,
        parallel_execution: parallel,
        parallel_workers: workers,
        desktop_host: desktop,
        imports: seeds,
        import_count: r.header.import_count,
        section_count: r.header.section_count,
        relocation_count: r.header.reloc_count,
    })
}
/// # Safety
/// The caller pins the exact reader/source for the entire blocking load and
/// supplies exclusive aligned output. Image storage is a new distinct range;
/// release is explicit only on the original failure edges, never by Drop.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_image_load(
    request: *const Request,
    out: *mut Loaded,
) -> bool {
    let r = unsafe { &*request };
    let mut raw = [Section::EMPTY; 16];
    if !sections(r, &mut raw) {
        log(b"Invalid R4M0 sections\r\n");
        return false;
    }
    let sections = &raw[..r.header.section_count as usize];
    let Some((v1, v2)) = exports(r, sections) else {
        log(b"Invalid R4M0 exports\r\n");
        return false;
    };
    let Some((memory, parallel, workers, desktop)) = metadata(r, v1, v2) else {
        return false;
    };
    let mut offsets = [0usize; 16];
    let Some(size) = layout(sections, &mut offsets) else {
        log(b"Invalid R4M0 image layout\r\n");
        return false;
    };
    let mut image = MaybeUninit::uninit();
    if !unsafe {
        super::stack::r4os_rust_program_image_allocate(size, r.owner_id, image.as_mut_ptr())
    } {
        log(b"Program module image allocation failed\r\n");
        return false;
    }
    let image = unsafe { image.assume_init() };
    unsafe { ptr::write_bytes(image.code, 0, image.len) };
    let Some(loaded) = finish(
        r, sections, &offsets, image, memory, parallel, workers, desktop,
    ) else {
        super::stack::r4os_rust_program_image_release(image.range_id);
        return false;
    };
    unsafe { out.write(loaded) };
    true
}
const _: () = {
    assert!(core::mem::size_of::<Reader>() == 16);
    assert!(core::mem::offset_of!(Reader, context) == 0);
    assert!(core::mem::offset_of!(Reader, read) == 8);
    assert!(core::mem::size_of::<Image>() == 32);
    assert!(core::mem::size_of::<ImportSeed>() == 112);
    assert!(core::mem::offset_of!(ImportSeed, module_name) == 32);
    assert!(core::mem::offset_of!(ImportSeed, module_name_len) == 64);
    assert!(core::mem::offset_of!(ImportSeed, symbol_name) == 72);
    assert!(core::mem::offset_of!(ImportSeed, symbol_name_len) == 104);
    assert!(core::mem::size_of::<Loaded>() == 1920);
    assert!(core::mem::offset_of!(Loaded, imports) == 112);
    assert!(core::mem::size_of::<Request>() == 96);
    assert!(core::mem::offset_of!(Request, file_size) == 80);
};

unsafe fn program_image_read(
    source: *const c_void,
    offset: usize,
    out: *mut u8,
    len: usize,
) -> bool {
    unsafe {
        crate::loader::file::r4os_rust_module_file_exact(
            source.cast(),
            offset,
            out,
            len,
            crate::fs::vfs_types::Bytes {
                data: b"r4x-section-payload".as_ptr(),
                len: 19,
            },
            true,
        )
    }
}
