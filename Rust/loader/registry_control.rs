//! Registry control, built-in providers, exact public lookups and original
//! diagnostics. The Contract's immutable provider metadata is projected by a
//! stateless peer; no second platform-name or Query table truth is kept here.
use super::{
    file::Source, format::ResolvedImport, registry_queries as query, registry_state as state,
};
use crate::fs::vfs_types::Bytes;
use core::{
    ptr, slice,
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
};

pub const UNKNOWN: usize = 0;
pub const RANGE: usize = 1;
pub const UNRESOLVED: usize = 2;
pub const MISSING: usize = 3;
pub const SYMBOL: usize = 4;
pub const VERSION: usize = 5;
pub const CYCLE: usize = 6;
pub const DUPLICATE: usize = 7;
static COUNTERS: [AtomicU32; 8] = [const { AtomicU32::new(0) }; 8];
static LIBRARIES_ACTIVE: AtomicBool = AtomicBool::new(false);
#[repr(C)]
pub struct Platform {
    pub name: Bytes,
    pub group: u32,
    pub query: u64,
}
#[repr(C)]
pub struct ExportInfo {
    pub address: u64,
    pub version: u32,
    pub available_size: u32,
    pub generation: u32,
    pub module_slot: u8,
    pub kind: u16,
}
#[repr(C)]
pub struct ResourceView {
    pub source: Source,
    pub file_offset: usize,
    pub catalog: *const super::resources::Catalog,
}

pub(crate) unsafe fn bytes<'a>(v: Bytes) -> &'a [u8] {
    if v.len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(v.data, v.len) }
    }
}
pub fn span(v: &[u8]) -> Bytes {
    Bytes {
        data: v.as_ptr(),
        len: v.len(),
    }
}
pub fn log(text: &[u8]) {
    unsafe { mod_log(0, 0, text.as_ptr(), text.len()) };
}
pub fn decimal(value: u64) {
    unsafe { mod_log(1, value, ptr::null(), 0) };
}
pub fn counter(index: usize) -> u32 {
    COUNTERS[index].load(Ordering::Relaxed)
}
pub fn add(index: usize) {
    COUNTERS[index]
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
        .expect("module error counter overflow");
}
fn copy(value: &[u8], out: &mut [u8]) -> usize {
    let n = value.len().min(out.len());
    out[..n].copy_from_slice(&value[..n]);
    n
}
pub(crate) unsafe fn view(p: *const state::Entry) -> query::EntryView {
    unsafe {
        query::EntryView {
            kind: (*p).kind,
            state: (*p).state,
            generation: (*p).generation,
            section_count: (*p).section_count,
            export_count: (*p).export_count,
            name: Bytes {
                data: ptr::addr_of!((*p).name).cast(),
                len: (*p).name_len,
            },
            sections: ptr::addr_of!((*p).sections).cast(),
            exports: ptr::addr_of!((*p).exports).cast(),
        }
    }
}
pub(crate) unsafe fn find_name(name: &[u8]) -> Option<usize> {
    for i in 0..state::capacity() {
        if !state::r4os_rust_mod_published(i) {
            continue;
        }
        let p = state::entry(i);
        let v = unsafe { view(p) };
        if unsafe { bytes(v.name) }.eq_ignore_ascii_case(name) {
            return Some(i);
        }
    }
    None
}
fn kind_name(kind: u16) -> &'static [u8] {
    match kind {
        1 => b"r4x",
        2 => b"r4l",
        3 => b"r4d",
        4 => b"r4p",
        5 => b"platform_api_provider_reserved",
        6 => b"kernel_module_reserved",
        _ => panic!("invalid module kind"),
    }
}
/// # Safety
/// Entry is the exact immutable published generation, retained through logging.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_publication(p: *const state::Entry) {
    let v = unsafe { view(p) };
    let mut record = crate::boot::record::Record::new();
    record.text(b"[MOD] load ");
    record.text(unsafe { bytes(v.name) });
    record.text(b" kind=");
    record.text(kind_name(v.kind));
    record.text(b" sections=");
    record.decimal(u64::from(v.section_count));
    record.text(b" image=0x");
    record.hex(unsafe { (*p).base });
    record.text(b" size=");
    record.decimal(unsafe { (*p).size as u64 });
    record.text(b" bss=");
    record.text(if unsafe { (*p).bss_zeroed } {
        b"zero"
    } else {
        b"bad"
    });
    if unsafe { (*p).pinned } {
        record.text(b" generation=");
        record.decimal(u64::from(v.generation));
        record.text(b" pinned");
    }
    if unsafe { (*p).import_count } != 0 {
        record.text(b" imports_resolved=");
        record.decimal(u64::from(unsafe { (*p).resolved_import_count }));
    }
    if unsafe { (*p).reloc_count } != 0 {
        record.text(b" relocs=");
        record.decimal(u64::from(unsafe { (*p).reloc_count }));
    }
    record.text(b"\r\n");
    log(record.as_bytes());
}
fn resolver_errors() {
    if [MISSING, SYMBOL, VERSION, CYCLE, DUPLICATE, UNRESOLVED]
        .iter()
        .all(|i| counter(*i) == 0)
    {
        return;
    }
    log(b"[MOD] resolver errors missing=");
    decimal(u64::from(counter(MISSING)));
    log(b" symbol=");
    decimal(u64::from(counter(SYMBOL)));
    log(b" version=");
    decimal(u64::from(counter(VERSION)));
    log(b" cycle=");
    decimal(u64::from(counter(CYCLE)));
    log(b" duplicate=");
    decimal(u64::from(counter(DUPLICATE)));
    log(b" unresolved=");
    decimal(u64::from(counter(UNRESOLVED)));
    log(b"\r\n");
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_mod_counter(index: u8, increment: bool) -> u32 {
    if increment {
        add(index as usize);
    }
    counter(index as usize)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_mod_libraries_active(publish: bool) -> bool {
    if publish {
        LIBRARIES_ACTIVE.store(true, Ordering::Release);
    }
    LIBRARIES_ACTIVE.load(Ordering::Acquire)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_mod_count() -> usize {
    (0..state::capacity())
        .filter(|i| state::r4os_rust_mod_published(*i))
        .count()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_mod_entry(index: usize) -> *const state::Entry {
    if state::r4os_rust_mod_published(index) {
        state::entry(index)
    } else {
        ptr::null()
    }
}
/// # Safety
/// Resident input name through this short, immutable registry lookup.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_find_name(name: Bytes) -> usize {
    unsafe { find_name(bytes(name)) }.unwrap_or(usize::MAX)
}
/// # Safety
/// Canonical platform peer returns exactly the six resident Contract records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_platform_group(name: Bytes) -> u64 {
    let name = unsafe { bytes(name) };
    for i in 0..6 {
        let mut meta = core::mem::MaybeUninit::<Platform>::uninit();
        if !unsafe { mod_platform(i, meta.as_mut_ptr()) } {
            return u64::MAX;
        }
        let meta = unsafe { meta.assume_init() };
        if unsafe { bytes(meta.name) }.eq_ignore_ascii_case(name) {
            return u64::from(meta.group);
        }
    }
    u64::MAX
}
/// # Safety
/// Original BSP bootstrap, with no live prior generation. Peers retain exact
/// heap/unwind/log contracts; projected metadata is immutable until shutdown.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_init(peers: &state::Peers) {
    unsafe { state::r4os_rust_mod_reset() };
    for counter in &COUNTERS {
        counter.store(0, Ordering::Relaxed);
    }
    LIBRARIES_ACTIVE.store(false, Ordering::Release);
    for i in 0..6 {
        let mut meta = core::mem::MaybeUninit::<Platform>::uninit();
        if !unsafe { mod_platform(i, meta.as_mut_ptr()) } {
            return;
        }
        let meta = unsafe { meta.assume_init() };
        let mut load = core::mem::MaybeUninit::<state::Prepared>::uninit();
        if !unsafe { state::r4os_rust_mod_reserve(load.as_mut_ptr(), peers) } {
            return;
        }
        let mut load = unsafe { load.assume_init() };
        let p = state::entry(load.slot);
        let mut e = state::Entry {
            kind: 5,
            state: 1,
            resolver_status: 1,
            export_count: 1,
            generation: load.generation,
            pinned: true,
            ..state::Entry::EMPTY
        };
        e.name_len = copy(unsafe { bytes(meta.name) }, &mut e.name);
        e.path_len = copy(b"builtin:", &mut e.path);
        e.path_len += copy(unsafe { bytes(meta.name) }, &mut e.path[e.path_len..]);
        e.exports[0].used = true;
        e.exports[0].version = 1;
        e.exports[0].address = meta.query;
        e.exports[0].name_len = copy(b"Query", &mut e.exports[0].name);
        unsafe { ptr::write(p, e) };
        let committed = unsafe { state::r4os_rust_mod_commit(&mut load, peers) };
        unsafe { state::r4os_rust_mod_abort(&mut load, peers) };
        if committed == usize::MAX {
            return;
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_mod_summary() {
    let mut counts = [0usize; 6];
    let mut failed = 0usize;
    for i in 0..state::capacity() {
        if !state::r4os_rust_mod_published(i) {
            continue;
        }
        let p = state::entry(i);
        let kind = unsafe { (*p).kind };
        match kind {
            1 => counts[1] += 1,
            2 => counts[0] += 1,
            3 => counts[2] += 1,
            4 => counts[3] += 1,
            5 | 6 => counts[4] += 1,
            _ => panic!("invalid module kind"),
        };
        if unsafe { (*p).state == 3 || (*p).resolver_status == 3 } {
            failed += 1;
        }
    }
    log(b"[MOD] summary total=");
    decimal(r4os_rust_mod_count() as u64);
    log(b" r4l=");
    decimal(counts[0] as u64);
    log(b" r4x=");
    decimal(counts[1] as u64);
    log(b" r4d=");
    decimal(counts[2] as u64);
    log(b" r4p=");
    decimal(counts[3] as u64);
    log(b" builtin=");
    decimal(counts[4] as u64);
    log(b" failed=");
    decimal(failed as u64);
    log(b"\r\n");
    if counter(UNKNOWN) != 0 || counter(RANGE) != 0 {
        log(b"[MOD] relocation errors unknown=");
        decimal(u64::from(counter(UNKNOWN)));
        log(b" range=");
        decimal(u64::from(counter(RANGE)));
        log(b"\r\n");
    }
    resolver_errors();
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_mod_library_results(loaded: usize) {
    log(b"[MOD] R4L loaded=");
    decimal(loaded as u64);
    log(b" total=");
    decimal(r4os_rust_mod_count() as u64);
    log(b"\r\n");
    resolver_errors();
}
/// # Safety
/// Entry and names retain the exact caller-owned generation. Out is aligned
/// writable private resolved-import storage, disjoint from the input tables.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_resolve_logged(
    p: *const state::Entry,
    module: Bytes,
    symbol: Bytes,
    min_version: u32,
    flags: u32,
    importer: Bytes,
    out: *mut ResolvedImport,
) -> bool {
    let view = unsafe { view(p) };
    let mut have = 0;
    let code = unsafe {
        query::r4os_rust_mod_resolve(&view, module, symbol, min_version, flags, out, &mut have)
    };
    match code {
        0 => return true,
        1 => return false,
        2 => {
            add(SYMBOL);
            log(b"[MOD] resolver missing export module=");
        }
        3 => {
            add(VERSION);
            log(b"[MOD] resolver version reject module=");
        }
        _ => unreachable!(),
    }
    log(unsafe { bytes(view.name) });
    log(b" symbol=");
    log(unsafe { bytes(symbol) });
    if code == 3 {
        log(b" have=");
        decimal(u64::from(have));
        log(b" need=");
        decimal(u64::from(min_version));
    }
    log(b" importer=");
    log(unsafe { bytes(importer) });
    log(b"\r\n");
    false
}
/// # Safety
/// Live exact entry generation. Logger projections may not mutate it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_required_logged(p: *const state::Entry) -> bool {
    let view = unsafe { view(p) };
    let result = unsafe { query::r4os_rust_mod_required(&view) };
    if result.reason.len != 0 {
        unsafe { reject(view.name, result.symbol, result.reason) };
    }
    result.accepted
}
/// # Safety
/// All message spans remain resident through the existing logger callbacks.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_reject(module: Bytes, symbol: Bytes, reason: Bytes) -> bool {
    unsafe { reject(module, symbol, reason) }
}
pub(crate) unsafe fn reject(module: Bytes, symbol: Bytes, reason: Bytes) -> bool {
    log(b"[MOD] interface reject module=");
    log(unsafe { bytes(module) });
    log(b" symbol=");
    log(unsafe { bytes(symbol) });
    log(b" reason=");
    log(unsafe { bytes(reason) });
    log(b"\r\n");
    false
}
/// # Safety
/// Resident query name and writable out; live claim publication authenticates
/// every referenced export/section before any available-size computation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_export_info(
    slot: usize,
    symbol: Bytes,
    min_version: u32,
    out: *mut ExportInfo,
) -> bool {
    if !state::r4os_rust_mod_published(slot) {
        return false;
    }
    let p = state::entry(slot);
    let view = unsafe { view(p) };
    let export = unsafe { query::r4os_rust_mod_find_export(&view, symbol) };
    if export.is_null() {
        return false;
    }
    let (address, version, index, offset) = unsafe {
        (
            (*export).address,
            (*export).version,
            (*export).section_index,
            (*export).section_offset,
        )
    };
    if version < min_version {
        return false;
    }
    let available_size = if view.kind == 5 {
        crate::contract::r4l_query_struct_size
    } else {
        if index >= view.section_count || index >= 16 {
            return false;
        }
        let section = unsafe { &*view.sections.add(index as usize) };
        if !section.used || section.flags & 1 == 0 || offset >= section.mem_size {
            return false;
        }
        section.mem_size - offset
    };
    unsafe {
        ptr::write(
            out,
            ExportInfo {
                address,
                version,
                available_size,
                generation: view.generation,
                module_slot: slot as u8,
                kind: view.kind,
            },
        )
    };
    true
}
/// # Safety
/// Resident names and writable ExportInfo output under the same lookup contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_resolve_info(
    module: Bytes,
    symbol: Bytes,
    min_version: u32,
    out: *mut ExportInfo,
) -> bool {
    let Some(slot) = (unsafe { find_name(bytes(module)) }) else {
        return false;
    };
    unsafe { r4os_rust_mod_export_info(slot, symbol, min_version, out) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_mod_code_owner(slot: u8, generation: u32, address: u64) -> bool {
    if generation == 0 || !state::r4os_rust_mod_published(slot as usize) {
        return false;
    }
    let view = unsafe { view(state::entry(slot as usize)) };
    unsafe { query::r4os_rust_mod_executable(&view, generation, address) }
}
/// # Safety
/// Out is writable storage for the exact generation-bound resource view. The
/// caller retains the same code/catalog lifetime as its original R4D binding.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_resources(
    slot: usize,
    generation: u32,
    out: *mut ResourceView,
) -> bool {
    if !state::r4os_rust_mod_published(slot) {
        return false;
    }
    let p = state::entry(slot);
    if unsafe { (*p).kind != 3 || (*p).generation != generation || !(*p).resource_source.present } {
        return false;
    }
    let source = unsafe { (*p).resource_source.value.assume_init_read() };
    let file_offset = unsafe { (*p).resource_file_offset };
    unsafe {
        ptr::write(
            out,
            ResourceView {
                source,
                file_offset,
                catalog: state::catalog(slot),
            },
        )
    };
    true
}
const _: () = {
    use core::mem::{offset_of, size_of};
    assert!(size_of::<Platform>() == 32);
    assert!(offset_of!(Platform, name) == 0);
    assert!(offset_of!(Platform, group) == 16);
    assert!(offset_of!(Platform, query) == 24);
    assert!(size_of::<ExportInfo>() == 24);
    assert!(offset_of!(ExportInfo, address) == 0);
    assert!(offset_of!(ExportInfo, version) == 8);
    assert!(offset_of!(ExportInfo, available_size) == 12);
    assert!(offset_of!(ExportInfo, generation) == 16);
    assert!(offset_of!(ExportInfo, module_slot) == 20);
    assert!(offset_of!(ExportInfo, kind) == 22);
    assert!(size_of::<ResourceView>() == 952);
    assert!(offset_of!(ResourceView, source) == 0);
    assert!(offset_of!(ResourceView, file_offset) == 936);
    assert!(offset_of!(ResourceView, catalog) == 944);
};

unsafe fn mod_log(op: u8, value: u64, data: *const u8, len: usize) {
    match op {
        0 => unsafe { crate::boot::log_route::r4os_rust_log_write(0, data, len) },
        1 => crate::boot::log_route::dec(value),
        2 => crate::boot::log_route::hex(value, 16),
        _ => unreachable!(),
    }
}

use crate::{build_config::config as kernel_config, contract as platform_contract};
const fn platform_queries() -> [platform_contract::R4LQuery; kernel_config::PLATFORM_API_COUNT] {
    let mut out = [const {
        platform_contract::R4LQuery {
            magic: platform_contract::r4l_abi_magic,
            abi_version: platform_contract::r4l_abi_version,
            size: platform_contract::r4l_query_struct_size,
            group: 0,
            kernel_bridge: 0,
            reserved: 0,
        }
    }; kernel_config::PLATFORM_API_COUNT];
    let mut i = 0;
    while i < out.len() {
        out[i].group = kernel_config::PLATFORM_APIS[i].1;
        i += 1;
    }
    out
}
static PLATFORM_QUERIES: [platform_contract::R4LQuery; kernel_config::PLATFORM_API_COUNT] =
    platform_queries();

unsafe fn mod_platform(index: usize, out: *mut Platform) -> bool {
    let Some(&(name, group)) = kernel_config::PLATFORM_APIS.get(index) else {
        return false;
    };
    unsafe {
        out.write(Platform {
            name: Bytes {
                data: name.as_ptr(),
                len: name.len(),
            },
            group,
            query: core::ptr::addr_of!(PLATFORM_QUERIES[index]) as u64,
        });
    }
    true
}

pub(crate) fn native_peers() -> state::Peers {
    unsafe extern "C" fn free(data: *mut u8, size: usize) -> u8 {
        u8::from(unsafe { crate::memory::heap::r4os_rust_heap_free(data, size) } != 0)
    }
    unsafe extern "C" fn log(data: *const u8, len: usize) {
        crate::boot::log_route::puts(unsafe { core::slice::from_raw_parts(data, len) });
    }
    state::Peers {
        scratch_size: super::registry_loader::r4os_rust_mod_scratch_layout(false),
        scratch_align: super::registry_loader::r4os_rust_mod_scratch_layout(true),
        alloc: crate::memory::heap::r4os_rust_heap_alloc,
        free,
        publication: r4os_rust_mod_publication,
        log,
    }
}
