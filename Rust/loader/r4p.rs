//! Original R4P preload/catalog/lazy activation owner. The single native
//! catalog remains resident; raw records retain existing serialized ownership
//! without a Rust state reference across file, callback, yield or log calls.
use super::{
    file, format, r4m, registry_control as ctl, registry_loader as loader, registry_state as image,
};
use crate::{
    contract as api,
    driver::{protocol_api as ppi, protocol_registry as registry},
    fs::{request, vfs, vfs_types as t},
    memory::heap,
    sched::scheduler,
};
use core::{
    cell::UnsafeCell,
    mem::{self, MaybeUninit},
    ptr, slice,
    sync::atomic::{AtomicU8, Ordering},
};
const N: usize = 32;
const DEP: usize = 16;
type Init = unsafe extern "C" fn(*const api::ProtocolApi) -> i32;
type Shutdown = unsafe extern "C" fn() -> i32;
type Query = unsafe extern "C" fn(*mut api::ProtocolStatus) -> i32;
type Dispatch =
    unsafe extern "C" fn(u32, *const api::ProtocolBuffer, *mut api::ProtocolBuffer) -> i32;
#[derive(Clone, Copy)]
struct Text<const C: usize> {
    data: [u8; C],
    len: usize,
}
impl<const C: usize> Text<C> {
    const EMPTY: Self = Self {
        data: [0; C],
        len: 0,
    };
    fn from(s: &[u8]) -> Self {
        let mut out = Self::EMPTY;
        let n = s.len().min(C);
        out.data[..n].copy_from_slice(&s[..n]);
        out.len = s.len();
        out
    }
    fn bytes(&self) -> &[u8] {
        &self.data[..self.len.min(C)]
    }
}
#[derive(Clone, Copy)]
struct Info {
    name: Text<32>,
    role: Text<64>,
    category: u16,
    dependencies: [Text<64>; DEP],
    dependency_count: usize,
}
impl Info {
    const EMPTY: Self = Self {
        name: Text::EMPTY,
        role: Text::EMPTY,
        category: 0,
        dependencies: [Text::EMPTY; DEP],
        dependency_count: 0,
    };
}
#[derive(Clone, Copy)]
struct Module {
    used: bool,
    initialized: bool,
    source: u8,
    _module_slot: usize,
    registry_slot: usize,
    info: Info,
    init: Option<Init>,
    _shutdown: Option<Shutdown>,
    query: Option<Query>,
    dispatch: Option<Dispatch>,
}
impl Module {
    const EMPTY: Self = Self {
        used: false,
        initialized: false,
        source: 1,
        _module_slot: 0,
        registry_slot: 0,
        info: Info::EMPTY,
        init: None,
        _shutdown: None,
        query: None,
        dispatch: None,
    };
}
#[derive(Clone, Copy)]
struct Candidate {
    used: bool,
    state: u8,
    source: Option<file::Source>,
    registry_slot: usize,
    module_index: Option<usize>,
    file_name: Text<64>,
    path: Text<96>,
    info: Info,
}
impl Candidate {
    const EMPTY: Self = Self {
        used: false,
        state: 0,
        source: None,
        registry_slot: 0,
        module_index: None,
        file_name: Text::EMPTY,
        path: Text::EMPTY,
        info: Info::EMPTY,
    };
}
struct Data {
    modules: [Module; N],
    candidates: [Candidate; N],
    initialized: bool,
    disk_catalog_loaded: bool,
    duplicates: usize,
    preload_headers: usize,
    cataloged_roles: usize,
    cataloged_candidates: usize,
    demand_loads: usize,
}
struct Owner(UnsafeCell<Data>);
// Original linear bootstrap and exact activation gate serialize this owner.
unsafe impl Sync for Owner {}
static DATA: Owner = Owner(UnsafeCell::new(Data {
    modules: [Module::EMPTY; N],
    candidates: [Candidate::EMPTY; N],
    initialized: false,
    disk_catalog_loaded: false,
    duplicates: 0,
    preload_headers: 0,
    cataloged_roles: 0,
    cataloged_candidates: 0,
    demand_loads: 0,
}));
static GATE: AtomicU8 = AtomicU8::new(0);

fn now() -> u64 {
    super::performance::now()
}
fn log(s: &[u8]) {
    ctl::log(s)
}
fn decimal(n: usize) {
    ctl::decimal(n as u64)
}
fn span(s: &[u8]) -> t::Bytes {
    ctl::span(s)
}
unsafe fn bytes<'a>(s: t::Bytes) -> &'a [u8] {
    unsafe { ctl::bytes(s) }
}
fn report(a: &[u8], b: &[u8], c: &[u8]) {
    log(a);
    log(b);
    log(c)
}
fn signed(value: i32) {
    if value < 0 {
        log(b"-");
        ctl::decimal(value.checked_neg().expect("R4P signed diagnostic overflow") as u64);
    } else {
        ctl::decimal(value as u64);
    }
}
fn md(i: usize) -> *mut Module {
    unsafe {
        ptr::addr_of_mut!((*DATA.0.get()).modules)
            .cast::<Module>()
            .add(i)
    }
}
fn cd(i: usize) -> *mut Candidate {
    unsafe {
        ptr::addr_of_mut!((*DATA.0.get()).candidates)
            .cast::<Candidate>()
            .add(i)
    }
}
fn same(a: &[u8], b: &[u8]) -> bool {
    a.eq_ignore_ascii_case(b)
}
fn extension(s: &[u8]) -> bool {
    s.len() >= 4 && s[s.len() - 4..].eq_ignore_ascii_case(b".r4p")
}
fn trim(s: &[u8]) -> &[u8] {
    if extension(s) { &s[..s.len() - 4] } else { s }
}
fn zlen(s: &[u8]) -> usize {
    s.iter().position(|v| *v == 0).unwrap_or(s.len())
}
fn source_name(source: u8) -> &'static [u8] {
    match source {
        0 => b"builtin",
        1 => b"r4p",
        2 => b"preload",
        _ => unreachable!(),
    }
}
fn module_source(source: u8) -> bool {
    matches!(source, 1 | 2)
}
fn role_state(state: u8) -> u8 {
    match state {
        1 => 1,
        2 | 3 => 2,
        4 => 3,
        5 => 4,
        6 => 5,
        _ => 0,
    }
}
fn free_module() -> Option<usize> {
    (0..N).find(|&i| unsafe { !(*md(i)).used })
}
fn free_candidate() -> Option<usize> {
    (0..N).find(|&i| unsafe { !(*cd(i)).used })
}
fn reg_get(slot: usize) -> Option<registry::Entry> {
    let mut out = MaybeUninit::<registry::Entry>::uninit();
    if unsafe { registry::r4os_rust_protocol_registry_get(slot, out.as_mut_ptr()) } {
        Some(unsafe { out.assume_init() })
    } else {
        None
    }
}
fn reg_entry(role: &[u8], mode: u8) -> Option<registry::Entry> {
    let mut out = MaybeUninit::<registry::Entry>::uninit();
    if unsafe {
        registry::r4os_rust_protocol_registry_entry(
            role.as_ptr(),
            role.len(),
            mode,
            out.as_mut_ptr(),
        )
    } {
        Some(unsafe { out.assume_init() })
    } else {
        None
    }
}
fn mutate(slot: usize, op: u8, state: u32, error: i32, note: &[u8]) {
    unsafe {
        registry::r4os_rust_protocol_registry_mutate(
            slot,
            op,
            state,
            error,
            note.as_ptr(),
            note.len(),
        )
    }
}
fn registration(info: &Info) -> registry::Registration {
    registry::Registration {
        name: info.name.data.as_ptr(),
        name_len: info.name.len,
        role: info.role.data.as_ptr(),
        role_len: info.role.len,
        note: b"".as_ptr(),
        note_len: 0,
        category: info.category,
        version: 1,
        api_version: api::protocol_api_version,
    }
}
fn register(op: u8, info: &Info) -> Option<usize> {
    let input = registration(info);
    let slot = unsafe { registry::r4os_rust_protocol_registry_register(op, &input) };
    if slot == usize::MAX { None } else { Some(slot) }
}
fn catalog_slot(role: &[u8]) -> Option<usize> {
    for i in 0..N {
        let p = cd(i);
        if unsafe { (*p).used } && same(unsafe { (*p).info.role.bytes() }, role) {
            return Some(unsafe { (*p).registry_slot });
        }
    }
    None
}
fn active(role: &[u8]) -> Option<usize> {
    for i in 0..N {
        let p = md(i);
        if unsafe { !(*p).used || !(*p).initialized } {
            continue;
        }
        if !same(unsafe { (*p).info.role.bytes() }, role) {
            continue;
        }
        if let Some(entry) = reg_get(unsafe { (*p).registry_slot })
            && module_source(entry.source)
            && entry.state == 2
        {
            return Some(i);
        }
    }
    None
}
fn count(state: u8) -> usize {
    (0..registry::MAX_PROTOCOLS)
        .filter(|&i| reg_get(i).is_some_and(|v| module_source(v.source) && v.state == state))
        .count()
}
fn refresh() {
    let roles = unsafe { (*DATA.0.get()).cataloged_roles };
    super::performance::record_r4p_results(roles, count(2), count(4), count(5));
}
fn ensure_initialized() {
    if unsafe { (*DATA.0.get()).initialized } {
        return;
    }
    registry::r4os_rust_protocol_registry_reset();
    for i in 0..N {
        unsafe {
            ptr::write(md(i), Module::EMPTY);
            ptr::write(cd(i), Candidate::EMPTY);
        }
    }
    unsafe {
        (*DATA.0.get()).preload_headers = 0;
        (*DATA.0.get()).disk_catalog_loaded = false;
        (*DATA.0.get()).duplicates = 0;
        (*DATA.0.get()).cataloged_roles = 0;
        (*DATA.0.get()).cataloged_candidates = 0;
        (*DATA.0.get()).demand_loads = 0;
    }
    GATE.store(0, Ordering::Relaxed);
    unsafe {
        (*DATA.0.get()).initialized = true;
    }
}
fn metadata_value<'a>(meta: &'a [u8], prefix: &[u8]) -> Option<&'a [u8]> {
    let s = unsafe { r4m::r4os_rust_r4m_meta(1, span(meta), span(prefix), ptr::null_mut()) };
    if s.present {
        Some(&meta[s.offset..s.offset + s.len])
    } else {
        None
    }
}
fn category(name: &[u8]) -> Option<u16> {
    if same(name, b"net") {
        Some(1)
    } else if same(name, b"usb") {
        Some(2)
    } else if same(name, b"audio") || same(name, b"synth") {
        Some(3)
    } else if same(name, b"data") {
        Some(4)
    } else if same(name, b"misc") || same(name, b"test") {
        Some(255)
    } else {
        None
    }
}
fn info_meta(meta: &[u8], default: &[u8]) -> Option<Info> {
    let name = metadata_value(meta, b"r4p.name=").unwrap_or_else(|| trim(default));
    let role = metadata_value(meta, b"r4p.role=")?;
    let mut info = Info {
        name: Text::from(name),
        role: Text::from(role),
        category: category(metadata_value(meta, b"r4p.category=").unwrap_or(b"misc"))?,
        ..Info::EMPTY
    };
    let mut offset = 0;
    loop {
        let s = unsafe { r4m::r4os_rust_r4m_meta(0, span(meta), span(b""), &mut offset) };
        if !s.present {
            break;
        }
        let item = &meta[s.offset..s.offset + s.len];
        if let Some(dep) = item.strip_prefix(b"r4p.dep=") {
            if info.dependency_count >= DEP {
                return None;
            }
            info.dependencies[info.dependency_count] = Text::from(dep);
            info.dependency_count += 1;
        }
    }
    Some(info)
}
fn info_bytes(data: &[u8], default: &[u8]) -> Option<Info> {
    if data.starts_with(b"R4P0") {
        log(b"[R4P] Legacy module format not supported\r\n");
        return None;
    }
    if data.len() < 64
        || !data.starts_with(b"R4M0")
        || u16::from_le_bytes(data[8..10].try_into().unwrap()) != 4
    {
        return None;
    }
    let off = u32::from_le_bytes(data[56..60].try_into().unwrap());
    let len = u32::from_le_bytes(data[60..64].try_into().unwrap());
    if !format::range(data.len(), off, len) {
        return None;
    }
    info_meta(&data[off as usize..][..len as usize], default)
}
fn info_file(source: *const file::Source, default: &[u8]) -> Option<Info> {
    let mut data = [0; 2048];
    let len = unsafe { loader::metadata_probe(source, 4, &mut data, b"r4p-metadata-probe") }?;
    info_meta(&data[..len], default)
}
fn too_long(name: &[u8], field: &[u8]) -> bool {
    report(b"[R4P] metadata ", field, b" too long: ");
    log(name);
    log(b"\r\n");
    false
}
fn fits(info: &Info, name: &[u8]) -> bool {
    if info.name.len > 32 {
        return too_long(name, b"name");
    }
    if info.role.len > 64 {
        return too_long(name, b"role");
    }
    if info.dependency_count > DEP {
        return too_long(name, b"dependency-list");
    }
    for dep in &info.dependencies[..info.dependency_count] {
        if dep.len > 64 {
            return too_long(name, b"dependency");
        }
    }
    true
}
fn missing_export(name: &[u8]) -> bool {
    report(b"[R4P] missing export ", name, b"\r\n");
    false
}
fn path(prefix: &[u8], name: &[u8]) -> Text<96> {
    let mut out = Text::EMPTY;
    let a = prefix.len().min(96);
    out.data[..a].copy_from_slice(&prefix[..a]);
    let b = name.len().min(96 - a);
    out.data[a..a + b].copy_from_slice(&name[..b]);
    out.len = a + b;
    out
}
fn mount() -> Option<t::Volume> {
    let mut out = MaybeUninit::<t::Volume>::uninit();
    if unsafe {
        vfs::r4os_rust_vfs_mount(4, b'C', ptr::null(), ptr::null(), out.as_mut_ptr().cast())
    } {
        Some(unsafe { out.assume_init() })
    } else {
        None
    }
}
fn request_begin() -> Option<request::Guard> {
    let mut out = MaybeUninit::<request::Guard>::uninit();
    if unsafe {
        request::r4os_rust_fs_request_begin(
            19,
            b'C',
            0,
            0,
            false,
            ptr::null(),
            0,
            false,
            out.as_mut_ptr(),
        )
    } {
        Some(unsafe { out.assume_init() })
    } else {
        None
    }
}
unsafe extern "C" fn alloc(size: usize, alignment: usize) -> *mut u8 {
    heap::r4os_rust_heap_alloc(size, alignment)
}
unsafe extern "C" fn free(data: *mut u8, len: usize) -> u8 {
    unsafe { heap::r4os_rust_heap_free(data, len) }
}
unsafe extern "C" fn load_log(data: *const u8, len: usize) {
    log(unsafe { slice::from_raw_parts(data, len) })
}
fn peers() -> image::Peers {
    image::Peers {
        scratch_size: loader::r4os_rust_mod_scratch_layout(false),
        scratch_align: loader::r4os_rust_mod_scratch_layout(true),
        alloc,
        free,
        publication: ctl::r4os_rust_mod_publication,
        log: load_log,
    }
}
struct Prepared {
    value: image::Prepared,
    peers: image::Peers,
}
impl Prepared {
    fn new(
        data: Option<&[u8]>,
        source: *const file::Source,
        name: &[u8],
        path: &[u8],
    ) -> Option<Self> {
        let peers = peers();
        let mut out = MaybeUninit::<image::Prepared>::uninit();
        if unsafe {
            loader::r4os_rust_mod_prepare(
                span(data.unwrap_or(b"")),
                source,
                4,
                span(name),
                span(path),
                &peers,
                out.as_mut_ptr(),
            )
        } {
            Some(Self {
                value: unsafe { out.assume_init() },
                peers,
            })
        } else {
            None
        }
    }
    fn export(&self, name: &[u8]) -> Option<u64> {
        let mut out = 0;
        if unsafe { loader::r4os_rust_mod_prepared_export(&self.value, span(name), 1, &mut out) } {
            Some(out)
        } else {
            missing_export(name);
            None
        }
    }
    fn commit(&mut self) -> Option<usize> {
        let slot = unsafe { image::r4os_rust_mod_commit(&mut self.value, &self.peers) };
        if slot == usize::MAX { None } else { Some(slot) }
    }
}
impl Drop for Prepared {
    fn drop(&mut self) {
        unsafe { image::r4os_rust_mod_abort(&mut self.value, &self.peers) };
    }
}
fn module(
    info: Info,
    source: u8,
    module_slot: usize,
    registry_slot: usize,
    exports: [u64; 4],
) -> Module {
    Module {
        used: true,
        source,
        _module_slot: module_slot,
        registry_slot,
        info,
        init: Some(unsafe { mem::transmute::<usize, Init>(exports[0] as usize) }),
        _shutdown: Some(unsafe { mem::transmute::<usize, Shutdown>(exports[1] as usize) }),
        query: Some(unsafe { mem::transmute::<usize, Query>(exports[2] as usize) }),
        dispatch: Some(unsafe { mem::transmute::<usize, Dispatch>(exports[3] as usize) }),
        ..Module::EMPTY
    }
}
fn load_bytes(data: &[u8], name: &[u8], path: &[u8], source: u8) -> bool {
    let Some(info) = info_bytes(data, name) else {
        report(b"[R4P] invalid R4M0 module: ", name, b"\r\n");
        return false;
    };
    load_bytes_info(data, info, name, path, source)
}
fn load_bytes_info(data: &[u8], info: Info, name: &[u8], path: &[u8], source: u8) -> bool {
    let Some(slot) = free_module() else {
        log(b"[R4P] module table full\r\n");
        return false;
    };
    if !fits(&info, name) {
        return false;
    }
    if reg_entry(info.role.bytes(), 1).is_some() {
        if source == 1 {
            unsafe {
                (*DATA.0.get()).duplicates += 1;
            }
        }
        report(b"[R4P] skip duplicate role ", info.role.bytes(), b" from ");
        log(name);
        log(b"\r\n");
        return false;
    }
    let Some(mut prepared) = Prepared::new(Some(data), ptr::null(), name, path) else {
        return false;
    };
    let Some(init) = prepared.export(b"ProtocolInit") else {
        return false;
    };
    let Some(shutdown) = prepared.export(b"ProtocolShutdown") else {
        return false;
    };
    let Some(query) = prepared.export(b"ProtocolQuery") else {
        return false;
    };
    let Some(dispatch) = prepared.export(b"ProtocolDispatch") else {
        return false;
    };
    let Some(registry_slot) = register(if source == 2 { 3 } else { 1 }, &info) else {
        report(
            b"[R4P] duplicate role or registry full: ",
            info.role.bytes(),
            b"\r\n",
        );
        return false;
    };
    let Some(module_slot) = prepared.commit() else {
        return false;
    };
    unsafe {
        ptr::write(
            md(slot),
            module(
                info,
                source,
                module_slot,
                registry_slot,
                [init, shutdown, query, dispatch],
            ),
        )
    };
    true
}
fn catalog(source: file::Source, name: &[u8]) -> bool {
    if source.entry.size == 0 {
        return false;
    }
    let start = now();
    let Some(info) = info_file(&source, name) else {
        super::performance::add_r4p_read_ticks(start);
        report(b"[R4P] invalid R4M0 module: ", name, b"\r\n");
        return false;
    };
    super::performance::add_r4p_read_ticks(start);
    if !fits(&info, name) {
        return false;
    }
    let role = info.role.bytes();
    let registry_slot = if let Some(existing) = reg_entry(role, 1) {
        if existing.source == 2 || existing.state != 1 {
            unsafe {
                (*DATA.0.get()).duplicates += 1;
            }
            report(b"[R4P] skip duplicate role ", role, b" from ");
            log(name);
            log(b"\r\n");
            return false;
        }
        let Some(slot) = catalog_slot(role) else {
            unsafe {
                (*DATA.0.get()).duplicates += 1;
            }
            return false;
        };
        unsafe {
            (*DATA.0.get()).duplicates += 1;
        }
        slot
    } else {
        let Some(slot) = register(2, &info) else {
            report(b"[R4P] registry full for role ", role, b"\r\n");
            return false;
        };
        unsafe {
            (*DATA.0.get()).cataloged_roles += 1;
        }
        slot
    };
    let Some(slot) = free_candidate() else {
        mutate(registry_slot, 1, 0, -3, b"lazy candidate table full");
        log(b"[R4P] candidate table full\r\n");
        return false;
    };
    let p = Candidate {
        used: true,
        source: Some(source),
        registry_slot,
        info,
        file_name: Text::from(&name[..name.len().min(64)]),
        path: path(b"C:\\R4OS\\PROTOCOLS\\", name),
        ..Candidate::EMPTY
    };
    unsafe {
        ptr::write(cd(slot), p);
        (*DATA.0.get()).cataloged_candidates += 1;
    }
    true
}
// Source and metadata copies belong only to loading, never to the recursive
// activation frame retained while its dependencies activate.
#[inline(never)]
fn load_candidate(index: usize) -> Option<usize> {
    if index >= N || unsafe { !(*cd(index)).used } {
        return None;
    }
    let p = cd(index);
    let source = unsafe { (*p).source }?;
    let info = unsafe { (*p).info };
    let name = unsafe { (*p).file_name };
    let path = unsafe { (*p).path };
    let Some(slot) = free_module() else {
        log(b"[R4P] module table full\r\n");
        return None;
    };
    if !fits(&info, name.bytes()) {
        return None;
    }
    let registry_slot = unsafe { (*p).registry_slot };
    let request = registration(&info);
    if !unsafe { registry::r4os_rust_protocol_registry_select(registry_slot, &request) } {
        return None;
    }
    unsafe {
        (*DATA.0.get()).demand_loads += 1;
    }
    report(b"[R4P] demand role=", info.role.bytes(), b" file=");
    log(name.bytes());
    log(b"\r\n");
    let start = now();
    let prepared = Prepared::new(None, &source, name.bytes(), path.bytes());
    super::performance::add_r4p_read_ticks(start);
    let mut prepared = prepared?;
    let init = prepared.export(b"ProtocolInit")?;
    let shutdown = prepared.export(b"ProtocolShutdown")?;
    let query = prepared.export(b"ProtocolQuery")?;
    let dispatch = prepared.export(b"ProtocolDispatch")?;
    let module_slot = prepared.commit()?;
    unsafe {
        ptr::write(
            md(slot),
            module(
                info,
                1,
                module_slot,
                registry_slot,
                [init, shutdown, query, dispatch],
            ),
        );
        (*p).module_index = Some(slot);
    }
    Some(slot)
}
fn module_role(index: usize) -> Text<64> {
    unsafe { (*md(index)).info.role }
}
fn dependency(index: usize, dep: usize) -> Text<64> {
    let p = md(index);
    if dep >= DEP || unsafe { dep >= (*p).info.dependency_count } {
        Text::EMPTY
    } else {
        unsafe { (*p).info.dependencies[dep] }
    }
}
fn init_module(index: usize) {
    let p = md(index);
    let name = unsafe { (*p).info.name };
    let role = unsafe { (*p).info.role };
    let source = unsafe { (*p).source };
    let slot = unsafe { (*p).registry_slot };
    report(b"[R4P] load ", name.bytes(), b" role=");
    log(role.bytes());
    log(b" source=");
    log(source_name(source));
    log(b"\r\n");
    let Some(init) = (unsafe { (*p).init }) else {
        mutate(slot, 1, 0, -4, b"missing init export");
        return;
    };
    let start = now();
    ppi::r4os_rust_protocol_slot(0, slot);
    let result = unsafe { init(ppi::r4os_rust_protocol_table()) };
    ppi::r4os_rust_protocol_slot(1, 0);
    super::performance::add_r4p_init_ticks(start);
    if result != 0 {
        mutate(slot, 1, 0, result, b"init failed");
        report(b"[R4P] init failed ", name.bytes(), b" code=");
        signed(result);
        log(b"\r\n");
        return;
    }
    let mut status = api::ProtocolStatus::default();
    let Some(query) = (unsafe { (*p).query }) else {
        mutate(slot, 0, 2, 0, b"");
        mutate(slot, 3, 0, 0, b"active; query export missing");
        return;
    };
    ppi::r4os_rust_protocol_slot(0, slot);
    let q = unsafe { query(&mut status) };
    ppi::r4os_rust_protocol_slot(1, 0);
    if q == 0 {
        mutate(slot, 4, status.state, 0, &status.note[..zlen(&status.note)]);
    } else {
        mutate(slot, 0, 2, 0, b"");
        mutate(slot, 3, 0, 0, b"active; query failed");
    }
}
fn dep_state(index: usize) -> u8 {
    let count = unsafe { (*md(index)).info.dependency_count };
    if count == 0 {
        return 0;
    }
    if count > DEP {
        return 2;
    }
    for i in 0..count {
        let dep = dependency(index, i);
        if dep.len == 0 {
            return 2;
        }
        let Some(entry) = reg_entry(dep.bytes(), 1) else {
            return 2;
        };
        match entry.state {
            2 => {}
            4..=6 => return 3,
            1 => return 1,
            _ => return 3,
        }
    }
    0
}
fn resolve_init() {
    let start = now();
    let mut progress = true;
    while progress {
        progress = false;
        for i in 0..N {
            let p = md(i);
            if unsafe { !(*p).used || (*p).initialized } {
                continue;
            }
            let state = dep_state(i);
            if state == 2 {
                mutate(
                    unsafe { (*p).registry_slot },
                    2,
                    0,
                    -5,
                    b"dependency missing",
                );
                unsafe {
                    (*p).initialized = true;
                }
                progress = true;
            } else if state == 3 {
                mutate(
                    unsafe { (*p).registry_slot },
                    2,
                    0,
                    -5,
                    b"dependency blocked",
                );
                unsafe {
                    (*p).initialized = true;
                }
                progress = true;
            } else if state == 0 {
                init_module(i);
                unsafe {
                    (*p).initialized = true;
                }
                progress = true;
            }
        }
    }
    for i in 0..N {
        let p = md(i);
        if unsafe { (*p).used && !(*p).initialized } {
            mutate(
                unsafe { (*p).registry_slot },
                2,
                0,
                -5,
                b"dependency cycle or unresolved dependency",
            );
            unsafe {
                (*p).initialized = true;
            }
        }
    }
    super::performance::add_r4p_resolve_ticks(start);
}
fn activate(index: usize, depth: usize) -> u8 {
    let p = cd(index);
    unsafe {
        (*p).state = 1;
    }
    let Some(slot) = load_candidate(index) else {
        unsafe {
            (*p).state = 3;
        }
        return 1;
    };
    let m = md(slot);
    let count = unsafe { (*m).info.dependency_count };
    for i in 0..count {
        let dep = dependency(slot, i);
        if dep.len == 0 || !ensure_serial(dep.bytes(), depth + 1) {
            let entry = reg_entry(dep.bytes(), 1);
            let note = match entry {
                None => b"dependency missing".as_slice(),
                Some(e) if e.state == 1 => b"dependency cycle or unresolved dependency".as_slice(),
                _ => b"dependency blocked".as_slice(),
            };
            mutate(unsafe { (*m).registry_slot }, 2, 0, -5, note);
            unsafe {
                (*m).initialized = true;
                (*p).state = 2;
            }
            refresh();
            return 2;
        }
    }
    init_module(slot);
    unsafe {
        (*m).initialized = true;
        (*p).state = 2;
    }
    refresh();
    if active(module_role(slot).bytes()).is_some() {
        0
    } else {
        2
    }
}
fn ensure_serial(role: &[u8], depth: usize) -> bool {
    if active(role).is_some() {
        return true;
    }
    if depth >= N {
        return false;
    }
    let mut saw_candidate = false;
    let mut saw_loading = false;
    for i in 0..N {
        let p = cd(i);
        if unsafe { !(*p).used } || !same(unsafe { (*p).info.role.bytes() }, role) {
            continue;
        }
        saw_candidate = true;
        match unsafe { (*p).state } {
            1 => saw_loading = true,
            2 => return active(role).is_some(),
            3 => continue,
            0 => match activate(i, depth) {
                0 => return true,
                2 => return false,
                1 => continue,
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
    }
    if saw_loading {
        return false;
    }
    if saw_candidate {
        if let Some(slot) = catalog_slot(role) {
            mutate(slot, 1, 0, -4, b"all lazy candidates invalid");
        }
        refresh();
    }
    false
}
struct Activation;
impl Activation {
    fn acquire() -> Self {
        while GATE
            .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            scheduler::yield_task();
        }
        Self
    }
}
impl Drop for Activation {
    fn drop(&mut self) {
        GATE.store(0, Ordering::Release);
    }
}
fn ensure_active(role: &[u8]) -> bool {
    if active(role).is_some() {
        return true;
    }
    let _gate = Activation::acquire();
    ensure_serial(role, 0)
}
fn load_all() {
    ensure_initialized();
    if unsafe { (*DATA.0.get()).disk_catalog_loaded } {
        refresh();
        return;
    }
    unsafe {
        (*DATA.0.get()).disk_catalog_loaded = true;
        (*DATA.0.get()).duplicates = 0;
        (*DATA.0.get()).cataloged_roles = 0;
        (*DATA.0.get()).cataloged_candidates = 0;
        (*DATA.0.get()).demand_loads = 0;
    }
    let Some(volume) = mount() else {
        log(b"[R4P] C: FAT32 volume not mounted\r\n");
        refresh();
        return;
    };
    let Some(mut req) = request_begin() else {
        return;
    };
    let result = unsafe {
        vfs::r4os_rust_vfs_run(
            0,
            &t::Args {
                volume,
                name: span(b"/R4OS/PROTOCOLS"),
                ..t::Args::default()
            },
            ptr::null_mut(),
        )
    };
    unsafe { request::r4os_rust_fs_request_finish(&mut req, result.status == 0) };
    if result.status != 0 {
        log(b"[R4P] /R4OS/PROTOCOLS not found\r\n");
        refresh();
        return;
    }
    let dir = result.value;
    log(b"[R4P] scanning /R4OS/PROTOCOLS\r\n");
    let mut index = 0;
    loop {
        let mut name = [0; 64];
        let Some(mut req) = request_begin() else {
            break;
        };
        let start = now();
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
        unsafe { request::r4os_rust_fs_request_finish(&mut req, true) };
        super::performance::add_r4p_scan_ticks(start);
        if result.status != 0 {
            break;
        }
        super::performance::record_r4p_scan_entry();
        let entry = unsafe { entry.assume_init() };
        let name = &name[..zlen(&name)];
        if !entry.is_dir() && extension(name) {
            super::performance::record_r4p_candidate();
            catalog(
                file::Source {
                    volume,
                    entry,
                    drive: b'C',
                },
                name,
            );
        }
        index += 1;
    }
    refresh();
    log(b"[R4P] catalog roles=");
    decimal(unsafe { (*DATA.0.get()).cataloged_roles });
    log(b" candidates=");
    decimal(unsafe { (*DATA.0.get()).cataloged_candidates });
    log(b" active=");
    decimal(count(2));
    log(b" demand-full=");
    decimal(unsafe { (*DATA.0.get()).demand_loads });
    let duplicates = unsafe { (*DATA.0.get()).duplicates };
    if duplicates != 0 {
        log(b" alternates=");
        decimal(duplicates);
    }
    log(b"\r\n");
}
#[repr(C)]
pub struct RoleStatus {
    pub active_r4p: bool,
    pub builtin_fallback: bool,
    pub state: u8,
}
/// # Safety
/// Original linear boot or first shell catalog admission; no concurrent reset.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4p_load_all() {
    load_all()
}
/// # Safety
/// Preload bytes/name remain resident through loader and protocol registration.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4p_preload(name: t::Bytes, data: t::Bytes) -> bool {
    ensure_initialized();
    let name = unsafe { bytes(name) };
    let path = path(b"PRELOAD:\\", name);
    if load_bytes(unsafe { bytes(data) }, name, path.bytes(), 2) {
        unsafe {
            (*DATA.0.get()).preload_headers += 1;
        }
        true
    } else {
        false
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_r4p_finish_preload() {
    ensure_initialized();
    resolve_init();
    log(b"[R4P] preload discovered=");
    decimal(unsafe { (*DATA.0.get()).preload_headers });
    log(b" active=");
    decimal(count(2));
    log(b"\r\n");
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_r4p_dump() {
    unsafe { r4p_dump() }
}
/// # Safety
/// Resident role and exact caller-owned protocol buffers through all callbacks.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4p_dispatch(
    role: t::Bytes,
    op: u32,
    input: *const api::ProtocolBuffer,
    output: *mut api::ProtocolBuffer,
) -> i32 {
    let role = unsafe { bytes(role) };
    if !ensure_active(role) {
        return -5;
    }
    let Some(slot) = active(role) else {
        return -5;
    };
    let m = md(slot);
    let Some(dispatch) = (unsafe { (*m).dispatch }) else {
        return -4;
    };
    let registry_slot = unsafe { (*m).registry_slot };
    ppi::r4os_rust_protocol_slot(0, registry_slot);
    let result = unsafe { dispatch(op, input, output) };
    ppi::r4os_rust_protocol_slot(1, 0);
    result
}
/// # Safety
/// Exact resident role through potential lazy I/O, dependency and module work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4p_has_active(role: t::Bytes) -> bool {
    ensure_active(unsafe { bytes(role) })
}
/// # Safety
/// Readable role for this passive selection only; no activation is performed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4p_source(role: t::Bytes, required: bool) -> t::Bytes {
    let Some(entry) = reg_entry(unsafe { bytes(role) }, 1) else {
        return span(if required { b"none" } else { b"fallback" });
    };
    span(if entry.state == 2 {
        source_name(entry.source)
    } else if required {
        b"none"
    } else {
        b"fallback"
    })
}
/// # Safety
/// Readable resident role; output is a copied passive C status snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4p_status(role: t::Bytes) -> RoleStatus {
    let role = unsafe { bytes(role) };
    let entry = reg_entry(role, 1);
    RoleStatus {
        active_r4p: active(role).is_some(),
        builtin_fallback: reg_entry(role, 0).is_some(),
        state: entry.map_or(0, |v| role_state(v.state)),
    }
}

unsafe fn r4p_dump() {
    crate::driver::protocol_registry::dump_status();
}
