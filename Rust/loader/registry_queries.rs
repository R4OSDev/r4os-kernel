//! Exact resident module export/import and executable-ownership policies.
//! Views reference the existing generation-bound registry without copying its
//! tables, retaining a lock or borrowing mutable state across external work.
use super::{format::ResolvedImport, r4m};
use crate::fs::vfs_types::Bytes;
use core::{ptr, slice};
#[repr(C)]
pub struct Export {
    pub used: bool,
    pub name: [u8; 32],
    pub name_len: usize,
    pub version: u32,
    pub address: u64,
    pub section_index: u32,
    pub section_offset: u32,
}
#[repr(C)]
pub struct Section {
    pub used: bool,
    pub name: [u8; 8],
    pub name_len: usize,
    pub flags: u32,
    pub file_size: u32,
    pub mem_size: u32,
    pub alignment: u32,
    pub runtime_offset: usize,
    pub runtime_base: u64,
}
#[repr(C)]
pub struct EntryView {
    pub kind: u16,
    pub state: u8,
    pub generation: u32,
    pub section_count: u32,
    pub export_count: u32,
    pub name: Bytes,
    pub sections: *const Section,
    pub exports: *const Export,
}
#[repr(C)]
pub struct Required {
    pub symbol: Bytes,
    pub reason: Bytes,
    pub accepted: bool,
}
unsafe fn bytes<'a>(value: Bytes) -> &'a [u8] {
    if value.len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(value.data, value.len) }
    }
}
fn span(value: &[u8]) -> Bytes {
    Bytes {
        data: value.as_ptr(),
        len: value.len(),
    }
}
fn required(symbol: &'static [u8], reason: &'static [u8], accepted: bool) -> Required {
    Required {
        symbol: span(symbol),
        reason: span(reason),
        accepted,
    }
}
unsafe fn find(entry: &EntryView, symbol: &[u8]) -> *const Export {
    for i in 0..(entry.export_count as usize).min(16) {
        let export = unsafe { &*entry.exports.add(i) };
        if export.used && export.name[..export.name_len].eq_ignore_ascii_case(symbol) {
            return export;
        }
    }
    ptr::null()
}
fn empty_import() -> ResolvedImport {
    ResolvedImport {
        module: [0; 32],
        module_len: 0,
        symbol: [0; 32],
        symbol_len: 0,
        version: 0,
        address: 0,
    }
}
fn copy(value: &[u8], out: &mut [u8; 32]) -> usize {
    let len = value.len().min(32);
    out[..len].copy_from_slice(&value[..len]);
    len
}
unsafe fn optional(module: Bytes, symbol: Bytes) -> ResolvedImport {
    let mut result = empty_import();
    result.module_len = copy(unsafe { bytes(module) }, &mut result.module);
    result.symbol_len = copy(unsafe { bytes(symbol) }, &mut result.symbol);
    result
}
/// # Safety
/// The exact generation's initialized immutable export array has16 slots;
/// the caller keeps registry/code ownership valid while using the raw result.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_find_export(
    entry: &EntryView,
    symbol: Bytes,
) -> *const Export {
    unsafe { find(entry, bytes(symbol)) }
}
/// # Safety
/// Entry names/tables are resident. Out is writable and disjoint; have_version
/// is written only for status3. No registry lock or wait crosses this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_resolve(
    entry: &EntryView,
    module: Bytes,
    symbol: Bytes,
    min_version: u32,
    flags: u32,
    out: *mut ResolvedImport,
    have_version: *mut u32,
) -> u8 {
    if !unsafe { r4m::r4os_rust_r4m_import(module, flags) } {
        return 1;
    }
    let is_optional = flags & 1 != 0;
    if is_optional && entry.kind != 2 {
        return 1;
    }
    let export = unsafe { find(entry, bytes(symbol)) };
    if export.is_null() {
        if is_optional {
            unsafe { ptr::write(out, optional(module, symbol)) };
            return 0;
        }
        return 2;
    }
    let export = unsafe { &*export };
    if export.version < min_version {
        if is_optional {
            unsafe { ptr::write(out, optional(module, symbol)) };
            return 0;
        }
        unsafe { ptr::write(have_version, export.version) };
        return 3;
    }
    let mut result = empty_import();
    result.module_len = copy(unsafe { bytes(entry.name) }, &mut result.module);
    result.symbol_len = copy(&export.name[..export.name_len], &mut result.symbol);
    result.version = export.version;
    result.address = export.address;
    unsafe { ptr::write(out, result) };
    0
}
/// # Safety
/// Input names are resident. Out is aligned writable result storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_optional(
    module: Bytes,
    symbol: Bytes,
    out: *mut ResolvedImport,
) {
    unsafe { ptr::write(out, optional(module, symbol)) };
}
/// # Safety
/// All len input records are initialized and resident for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_resolved_count(
    imports: *const ResolvedImport,
    len: usize,
) -> u32 {
    let mut count = 0u32;
    for i in 0..len {
        if unsafe { (*imports.add(i)).address } != 0 {
            count += 1;
        }
    }
    count
}
/// # Safety
/// The caller validated the live claim/generation before constructing entry.
/// Its16 section records remain resident and immutable through this lookup.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_executable(
    entry: &EntryView,
    generation: u32,
    address: u64,
) -> bool {
    if entry.state != 2 || entry.kind != 2 || entry.generation != generation {
        return false;
    }
    for i in 0..(entry.section_count as usize).min(16) {
        let section = unsafe { &*entry.sections.add(i) };
        if !section.used || section.flags & 3 != 3 || section.mem_size == 0 {
            continue;
        }
        let end = section
            .runtime_base
            .wrapping_add(u64::from(section.mem_size));
        if end < section.runtime_base {
            continue;
        }
        if address >= section.runtime_base && address < end {
            return true;
        }
    }
    false
}
/// # Safety
/// Initialized entry data has16 section/export slots; bounded admitted section
/// indices and names obey the existing private loader contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_required(entry: &EntryView) -> Required {
    let symbols: &[&'static [u8]] = match entry.kind {
        3 => &[b"DriverInit", b"DriverShutdown"],
        4 => &[
            b"ProtocolInit",
            b"ProtocolShutdown",
            b"ProtocolQuery",
            b"ProtocolDispatch",
        ],
        _ => return required(b"", b"", true),
    };
    for &symbol in symbols {
        let export = unsafe { find(entry, symbol) };
        if export.is_null() {
            return required(symbol, b"required export missing", false);
        }
        let export = unsafe { &*export };
        if export.version < 1 || export.address == 0 || export.section_index >= entry.section_count
        {
            return required(b"", b"", false);
        }
        let sections = unsafe { slice::from_raw_parts(entry.sections, 16) };
        let section = &sections[export.section_index as usize];
        if section.flags & 2 == 0 || export.section_offset >= section.mem_size {
            return required(symbol, b"required export is not executable", false);
        }
    }
    required(b"", b"", true)
}
const _: () = {
    use core::mem::{offset_of, size_of};
    assert!(size_of::<Export>() == 72);
    assert!(offset_of!(Export, used) == 0);
    assert!(offset_of!(Export, name) == 1);
    assert!(offset_of!(Export, name_len) == 40);
    assert!(offset_of!(Export, version) == 48);
    assert!(offset_of!(Export, address) == 56);
    assert!(offset_of!(Export, section_index) == 64);
    assert!(offset_of!(Export, section_offset) == 68);
    assert!(size_of::<Section>() == 56);
    assert!(offset_of!(Section, used) == 0);
    assert!(offset_of!(Section, name) == 1);
    assert!(offset_of!(Section, name_len) == 16);
    assert!(offset_of!(Section, flags) == 24);
    assert!(offset_of!(Section, file_size) == 28);
    assert!(offset_of!(Section, mem_size) == 32);
    assert!(offset_of!(Section, alignment) == 36);
    assert!(offset_of!(Section, runtime_offset) == 40);
    assert!(offset_of!(Section, runtime_base) == 48);
    assert!(size_of::<EntryView>() == 48);
    assert!(offset_of!(EntryView, kind) == 0);
    assert!(offset_of!(EntryView, state) == 2);
    assert!(offset_of!(EntryView, generation) == 4);
    assert!(offset_of!(EntryView, section_count) == 8);
    assert!(offset_of!(EntryView, export_count) == 12);
    assert!(offset_of!(EntryView, name) == 16);
    assert!(offset_of!(EntryView, sections) == 32);
    assert!(offset_of!(EntryView, exports) == 40);
    assert!(size_of::<Required>() == 40);
    assert!(offset_of!(Required, symbol) == 0);
    assert!(offset_of!(Required, reason) == 16);
    assert!(offset_of!(Required, accepted) == 32);
};
