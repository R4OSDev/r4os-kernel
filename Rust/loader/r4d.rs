//! Original R4D discovery, activation and terminal lifecycle owner.
//! State stays under the exact native unwind guard. No Rust state borrow
//! survives a wait, logger, file operation or external driver callback.
use super::{
    file, format, registry_control as ctl, registry_loader as loader, registry_state as modules,
};
use crate::driver::{
    owner_storage,
    owner_table::{Empty, Table},
};
use crate::{
    contract as api,
    driver::{api_admission as admission, api_cleanup as cleanup, registry},
    fs::{request, vfs, vfs_types as t},
    memory::heap,
    platform::timer,
    sched::{
        sync_locks,
        sync_types::{Text, UnwindGuard, WaitQueue},
        wait_node::QueueCore,
    },
};
use core::{
    cell::UnsafeCell,
    mem::{self, MaybeUninit},
    ptr, slice,
};
impl Empty for Runtime {
    const EMPTY: Self = Self::EMPTY;
}
impl Empty for Module {
    const EMPTY: Self = Self::EMPTY;
}
const LOADED: u8 = 0;
const ALREADY: u8 = 1;
const NAME_INVALID: u8 = 2;
const FILE_MISSING: u8 = 3;
const INVALID_FILE: u8 = 4;
const INIT_FAILED: u8 = 5;
const LOAD_FAILED: u8 = 6;
type Init = unsafe extern "C" fn(*const api::DriverApi) -> i32;
type Shutdown = unsafe extern "C" fn() -> i32;
#[derive(Clone, Copy)]
struct Name {
    data: [u8; 32],
    len: usize,
}
impl Name {
    const EMPTY: Self = Self {
        data: [0; 32],
        len: 0,
    };
    fn full(s: &[u8]) -> Self {
        let mut n = Self::EMPTY;
        n.len = s.len().min(32);
        n.data[..n.len].copy_from_slice(&s[..n.len]);
        n
    }
    fn stored(s: &[u8]) -> Self {
        Self::full(&s[..s.len().min(31)])
    }
    fn bytes(&self) -> &[u8] {
        &self.data[..self.len]
    }
}
#[derive(Clone, Copy)]
struct Runtime {
    used: bool,
    quarantined: bool,
    generation: u64,
    name: Name,
    kind: u16,
    registry_slot: usize,
    module_slot: usize,
    owner: u32,
    shutdown: Option<Shutdown>,
}
impl Runtime {
    const EMPTY: Self = Self {
        used: false,
        quarantined: false,
        generation: 0,
        name: Name::EMPTY,
        kind: 0,
        registry_slot: 0,
        module_slot: 0,
        owner: 0,
        shutdown: None,
    };
}
#[derive(Clone, Copy)]
struct Module {
    used: bool,
    quarantined: bool,
    name: Name,
    kind: u16,
    parallel_callbacks: bool,
    module_slot: usize,
    init: Option<Init>,
    shutdown: Option<Shutdown>,
}
impl Module {
    const EMPTY: Self = Self {
        used: false,
        quarantined: false,
        name: Name::EMPTY,
        kind: 0,
        parallel_callbacks: false,
        module_slot: 0,
        init: None,
        shutdown: None,
    };
}
struct Data {
    runtime: Table<Runtime>,
    modules: Table<Module>,
    next_generation: u64,
    transition: bool,
}
struct Owner(UnsafeCell<Data>);
// The original reentrant lifecycle guard owns every field and exact instance.
unsafe impl Sync for Owner {}
static DATA: Owner = Owner(UnsafeCell::new(Data {
    runtime: Table::new(),
    modules: Table::new(),
    next_generation: 1,
    transition: false,
}));
struct Guard(UnsafeCell<UnwindGuard>);
unsafe impl Sync for Guard {}
static GUARD: Guard = Guard(UnsafeCell::new(UnwindGuard {
    owner: 0,
    owner_generation: 0,
    boot_owner: false,
    depth: 0,
    name: Text::literal(b"r4d-lifecycle"),
    queue: WaitQueue {
        core: QueueCore::EMPTY,
    },
    ..UnwindGuard::EMPTY
}));

fn span(s: &[u8]) -> t::Bytes {
    ctl::span(s)
}
unsafe fn bytes<'a>(s: t::Bytes) -> &'a [u8] {
    unsafe { ctl::bytes(s) }
}
fn log(s: &[u8]) {
    ctl::log(s)
}
fn decimal(v: u64) {
    ctl::decimal(v)
}
fn signed(v: i32) {
    let n = i64::from(v);
    if n < 0 {
        log(b"-");
        decimal((-n) as u64);
    } else {
        decimal(n as u64);
    }
}
fn report(prefix: &[u8], name: &[u8], suffix: &[u8]) {
    log(prefix);
    log(name);
    log(suffix);
}
fn runtimes() -> *mut Table<Runtime> {
    unsafe { ptr::addr_of_mut!((*DATA.0.get()).runtime) }
}
fn module_rows() -> *mut Table<Module> {
    unsafe { ptr::addr_of_mut!((*DATA.0.get()).modules) }
}
fn runtime_capacity() -> usize {
    unsafe { (*runtimes()).capacity() }
}
fn module_capacity() -> usize {
    unsafe { (*module_rows()).capacity() }
}
fn rt(index: usize) -> *mut Runtime {
    unsafe { Table::get(runtimes(), index) }
}
fn md(index: usize) -> *mut Module {
    unsafe { Table::get(module_rows(), index) }
}
fn handoff() -> u64 {
    (u64::from(timer::frequency()) * 250).div_ceil(1000).max(1)
}
fn leave() {
    unsafe { sync_locks::r4os_rust_sync_guard(GUARD.0.get(), 3, 0) };
}
struct Life;
impl Life {
    fn enter(timeout: u64, terminal: bool) -> Option<Self> {
        if !unsafe { sync_locks::r4os_rust_sync_guard(GUARD.0.get(), 2, timeout) } {
            return None;
        }
        if !terminal && unsafe { (*DATA.0.get()).transition } {
            leave();
            return None;
        }
        Some(Self)
    }
}
impl Drop for Life {
    fn drop(&mut self) {
        leave();
    }
}
fn find_runtime(name: &[u8]) -> Option<usize> {
    for i in 0..runtime_capacity() {
        let p = rt(i);
        if unsafe { (*p).used } && same(unsafe { (*p).name.bytes() }, name) {
            return Some(i);
        }
    }
    None
}
fn find_module(name: &[u8]) -> Option<usize> {
    for i in 0..module_capacity() {
        let p = md(i);
        if unsafe { (*p).used } && same(unsafe { (*p).name.bytes() }, name) {
            return Some(i);
        }
    }
    None
}
fn free_runtime() -> Option<usize> {
    loop {
        let capacity = runtime_capacity();
        if let Some(i) = (0..capacity).find(|&i| unsafe { !(*rt(i)).used }) {
            return Some(i);
        }
        if !owner_storage::runtime(runtimes(), capacity) {
            return None;
        }
    }
}
fn free_module() -> Option<usize> {
    loop {
        let capacity = module_capacity();
        if let Some(i) = (0..capacity).find(|&i| unsafe { !(*md(i)).used }) {
            return Some(i);
        }
        if !owner_storage::runtime(module_rows(), capacity) {
            return None;
        }
    }
}
fn generation() -> u64 {
    unsafe {
        let p = ptr::addr_of_mut!((*DATA.0.get()).next_generation);
        let n = p.read();
        let next = n.wrapping_add(1);
        p.write(if next == 0 { 1 } else { next });
        n
    }
}
fn set_state(slot: usize, state: u8) {
    registry::r4os_rust_driver_registry_set_state(slot, state)
}
fn identity(p: *const Runtime, generation: u64, owner: u32, slot: usize) -> bool {
    unsafe {
        (*p).used
            && (*p).generation == generation
            && (*p).owner == owner
            && (*p).registry_slot == slot
    }
}
fn active(raw: &[u8]) -> bool {
    let name = trim(raw);
    let slot = unsafe { registry::r4os_rust_driver_registry_find(name.as_ptr(), name.len()) };
    if slot == usize::MAX {
        return false;
    }
    let mut out = MaybeUninit::<registry::Entry>::uninit();
    if !unsafe { registry::r4os_rust_driver_registry_get(slot, out.as_mut_ptr()) } {
        return false;
    }
    let e = unsafe { out.assume_init() };
    if !(1..=3).contains(&e.state) {
        return false;
    }
    report(b"[R4D] already active: ", &e.name[..e.name_len], b" (");
    log(match e.source {
        0 => b"builtin",
        1 => b"r4d",
        2 => b"preload",
        _ => unreachable!(),
    });
    log(b"/");
    log(match e.state {
        1 => b"GELADEN",
        2 => b"INITIALISIERT",
        3 => b"AKTIV",
        _ => unreachable!(),
    });
    log(b")\r\n");
    true
}
fn trim(s: &[u8]) -> &[u8] {
    if extension(s) { &s[..s.len() - 4] } else { s }
}
fn same(a: &[u8], b: &[u8]) -> bool {
    a.eq_ignore_ascii_case(trim(b))
}
fn extension(s: &[u8]) -> bool {
    s.len() >= 4 && s[s.len() - 4..].eq_ignore_ascii_case(b".r4d")
}
fn fallback(s: &[u8]) -> &[u8] {
    trim(
        &s[s.iter()
            .rposition(|b| *b == b'/' || *b == b'\\')
            .map_or(0, |i| i + 1)..],
    )
}
fn zlen(s: &[u8]) -> usize {
    s.iter().position(|b| *b == 0).unwrap_or(s.len())
}
fn magic(s: &[u8]) -> bool {
    s.starts_with(b"R4M0")
}
fn path<'a>(prefix: &[u8], name: &[u8], out: &'a mut [u8; 96]) -> &'a [u8] {
    let a = prefix.len().min(out.len());
    out[..a].copy_from_slice(&prefix[..a]);
    let b = name.len().min(out.len() - a);
    out[a..a + b].copy_from_slice(&name[..b]);
    &out[..a + b]
}
fn driver_path<'a>(name: &[u8], out: &'a mut [u8; 96]) -> Option<&'a [u8]> {
    let name = trim(name);
    let prefix = b"/R4OS/DRIVERS/";
    let suffix = b".r4d";
    let end = prefix.len() + name.len();
    if name.is_empty() || end + suffix.len() > out.len() {
        return None;
    }
    out[..prefix.len()].copy_from_slice(prefix);
    for (i, b) in name.iter().enumerate() {
        out[prefix.len() + i] = b.to_ascii_lowercase();
    }
    out[end..end + 4].copy_from_slice(suffix);
    Some(&out[..end + 4])
}
fn type_name(kind: u16) -> &'static [u8] {
    match kind {
        1 => b"audio",
        2 => b"storage",
        3 => b"input",
        4 => b"synth",
        5 => b"net",
        6 => b"display",
        255 => b"misc",
        _ => b"unknown",
    }
}
fn type_parse(name: &[u8]) -> Option<u16> {
    [1, 2, 3, 4, 5, 6, 255]
        .into_iter()
        .find(|&i| same(name, type_name(i)))
}
struct Info {
    name: Name,
    kind: u16,
    parallel_callbacks: bool,
}
fn meta_value<'a>(meta: &'a [u8], prefix: &[u8]) -> Option<&'a [u8]> {
    let mut offset = 0;
    let s = unsafe { super::r4m::r4os_rust_r4m_meta(1, span(meta), span(prefix), &mut offset) };
    if s.present {
        Some(&meta[s.offset..s.offset + s.len])
    } else {
        None
    }
}
fn info_meta(meta: &[u8], default: &[u8]) -> Option<Info> {
    let name = meta_value(meta, b"r4d.name=").unwrap_or_else(|| trim(default));
    let kind = meta_value(meta, b"r4d.type=").unwrap_or(b"misc");
    let Some(kind) = type_parse(kind) else {
        log(b"[R4D] invalid R4M0 driver type\r\n");
        return None;
    };
    let parallel_callbacks = match meta_value(meta, b"r4d.parallel_callbacks=") {
        None => false,
        Some(b"owned-v1") => true,
        Some(_) => {
            log(b"[R4D] invalid callback ownership declaration\r\n");
            return None;
        }
    };
    Some(Info {
        name: Name::full(name),
        kind,
        parallel_callbacks,
    })
}
fn info_bytes(data: &[u8], default: &[u8]) -> Option<Info> {
    if !magic(data) || data.len() < 64 {
        return None;
    }
    if u16::from_le_bytes(data[8..10].try_into().unwrap()) != 3 {
        log(b"[R4D] R4M0 kind is not r4d\r\n");
        return None;
    }
    let off = u32::from_le_bytes(data[56..60].try_into().unwrap());
    let len = u32::from_le_bytes(data[60..64].try_into().unwrap());
    if !format::range(data.len(), off, len) {
        log(b"[R4D] invalid R4M0 metadata\r\n");
        return None;
    }
    info_meta(&data[off as usize..][..len as usize], default)
}
fn info_file(source: *const file::Source, default: &[u8]) -> Option<Info> {
    let mut data = [0; 2048];
    let len = unsafe { loader::metadata_probe(source, 3, &mut data, b"r4d-metadata-probe") }?;
    info_meta(&data[..len], default)
}
fn begin_request() -> Option<request::Guard> {
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
fn read_file(name: &[u8]) -> Result<file::Source, u8> {
    let Some(volume) = mount() else {
        log(b"[R4D] C: FAT32 volume not mounted\r\n");
        return Err(1);
    };
    let Some(mut req) = begin_request() else {
        return Err(5);
    };
    let mut entry = MaybeUninit::<t::Entry>::uninit();
    let result = unsafe {
        vfs::r4os_rust_vfs_run(
            1,
            &t::Args {
                volume,
                name: span(name),
                ..t::Args::default()
            },
            entry.as_mut_ptr(),
        )
    };
    if result.status != 0 {
        unsafe { request::r4os_rust_fs_request_finish(&mut req, false) };
        report(b"[R4D] driver not found: ", name, b"\r\n");
        return Err(2);
    }
    let entry = unsafe { entry.assume_init() };
    if entry.is_dir() || entry.size == 0 {
        unsafe { request::r4os_rust_fs_request_finish(&mut req, false) };
        log(b"[R4D] invalid driver path\r\n");
        return Err(3);
    }
    unsafe { request::r4os_rust_fs_request_finish(&mut req, true) };
    Ok(file::Source {
        volume,
        entry,
        drive: b'C',
    })
}
unsafe extern "C" fn alloc(size: usize, alignment: usize) -> *mut u8 {
    heap::r4os_rust_heap_alloc(size, alignment)
}
unsafe extern "C" fn free(data: *mut u8, size: usize) -> u8 {
    unsafe { heap::r4os_rust_heap_free(data, size) }
}
unsafe extern "C" fn load_log(data: *const u8, len: usize) {
    log(unsafe { slice::from_raw_parts(data, len) })
}
fn peers() -> modules::Peers {
    modules::Peers {
        scratch_size: loader::r4os_rust_mod_scratch_layout(false),
        scratch_align: loader::r4os_rust_mod_scratch_layout(true),
        alloc,
        free,
        publication: ctl::r4os_rust_mod_publication,
        log: load_log,
    }
}
struct Prepared {
    value: modules::Prepared,
    peers: modules::Peers,
}
impl Prepared {
    fn new(
        data: Option<&[u8]>,
        source: *const file::Source,
        default: &[u8],
        name: &[u8],
    ) -> Option<Self> {
        let peers = peers();
        let mut out = MaybeUninit::<modules::Prepared>::uninit();
        if unsafe {
            loader::r4os_rust_mod_prepare(
                span(data.unwrap_or(b"")),
                source,
                3,
                span(default),
                span(name),
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
    fn export(&self, symbol: &[u8]) -> Option<u64> {
        let mut out = 0;
        if unsafe { loader::r4os_rust_mod_prepared_export(&self.value, span(symbol), 1, &mut out) }
        {
            Some(out)
        } else {
            None
        }
    }
    fn commit(&mut self) -> Option<usize> {
        let slot = unsafe { modules::r4os_rust_mod_commit(&mut self.value, &self.peers) };
        if slot == usize::MAX { None } else { Some(slot) }
    }
}
impl Drop for Prepared {
    fn drop(&mut self) {
        unsafe { modules::r4os_rust_mod_abort(&mut self.value, &self.peers) };
    }
}
fn descriptor(
    info: &Info,
    data: Option<&[u8]>,
    source: *const file::Source,
    default: &[u8],
    name: &[u8],
    runtime: bool,
) -> Result<usize, u8> {
    if let Some(slot) = find_module(info.name.bytes()) {
        return Ok(slot);
    }
    let mut prepared = Prepared::new(data, source, default, name).ok_or(LOAD_FAILED)?;
    let Some(init) = prepared.export(b"DriverInit") else {
        log(b"[R4D] missing DriverInit export\r\n");
        return Err(if runtime { INVALID_FILE } else { LOAD_FAILED });
    };
    let Some(shutdown) = prepared.export(b"DriverShutdown") else {
        log(b"[R4D] missing DriverShutdown export\r\n");
        return Err(if runtime { INVALID_FILE } else { LOAD_FAILED });
    };
    let Some(slot) = free_module() else {
        log(b"[R4D] module-driver metadata memory exhausted\r\n");
        return Err(LOAD_FAILED);
    };
    let module_slot = prepared.commit().ok_or(LOAD_FAILED)?;
    unsafe {
        ptr::write(
            md(slot),
            Module {
                used: true,
                name: Name::stored(info.name.bytes()),
                kind: info.kind,
                parallel_callbacks: info.parallel_callbacks,
                module_slot,
                init: Some(mem::transmute::<usize, Init>(init as usize)),
                shutdown: Some(mem::transmute::<usize, Shutdown>(shutdown as usize)),
                ..Module::EMPTY
            },
        )
    };
    Ok(slot)
}
fn load_file(source: *const file::Source, default: &[u8], name: &[u8], runtime: bool) -> u8 {
    let Some(info) = info_file(source, default) else {
        return INVALID_FILE;
    };
    let slot = match descriptor(&info, None, source, default, name, runtime) {
        Ok(slot) => slot,
        Err(error) => return error,
    };
    if !runtime {
        let name = unsafe { (*md(slot)).name };
        report(b"[R4D] module ", name.bytes(), b" recognized\r\n");
        return LOADED;
    }
    init_module(slot, 1)
}
fn preload(data: &[u8], default: &[u8], name: &[u8]) -> u8 {
    let Some(info) = info_bytes(data, default) else {
        return INVALID_FILE;
    };
    let slot = match descriptor(&info, Some(data), ptr::null(), default, name, true) {
        Ok(slot) => slot,
        Err(error) => return error,
    };
    init_module(slot, 2)
}
fn failed_log(name: &[u8], owner: u32, reason: &[u8]) {
    report(
        b"[R4D] failed-load resources quarantined name=",
        name,
        b" owner=",
    );
    decimal(u64::from(owner));
    log(b" reason=");
    log(reason);
    log(b"\r\n");
}
fn failed_cleanup(
    shutdown: Shutdown,
    owner: u32,
    name: &[u8],
    descriptor: *mut Module,
    slot: usize,
) -> bool {
    unsafe { (*descriptor).quarantined = true };
    set_state(slot, 7);
    if !admission::r4os_rust_driver_api_enter(owner, handoff()) {
        failed_log(name, owner, b"owner-timeout");
        return false;
    }
    let mut out = MaybeUninit::<cleanup::Token>::uninit();
    if !unsafe { cleanup::r4os_rust_driver_cleanup_prepare(owner, out.as_mut_ptr()) } {
        admission::r4os_rust_driver_api_leave();
        failed_log(name, owner, b"resource-busy");
        return false;
    }
    let mut token = unsafe { out.assume_init() };
    let input = unsafe { (*descriptor).kind == 3 };
    if !input {
        unsafe { cleanup::r4os_rust_driver_cleanup_begin(&mut token) }
    }
    let result = unsafe { shutdown() };
    if input {
        unsafe { cleanup::r4os_rust_driver_cleanup_begin(&mut token) }
    }
    if result != 0 {
        unsafe { cleanup::r4os_rust_driver_cleanup_quarantine(&mut token) };
        admission::r4os_rust_driver_api_leave();
        report(b"[R4D] failed-load shutdown failed name=", name, b" owner=");
        decimal(u64::from(owner));
        log(b" code=");
        signed(result);
        log(b" resources=quarantined\r\n");
        return false;
    }
    let ok = unsafe { cleanup::r4os_rust_driver_cleanup_commit(&mut token) };
    if !ok {
        admission::r4os_rust_driver_api_leave();
        failed_log(name, owner, b"cleanup-failed");
        return false;
    }
    admission::r4os_rust_driver_api_leave();
    true
}
fn init_module(slot: usize, source: u8) -> u8 {
    let p = md(slot);
    let name = unsafe { (*p).name };
    if unsafe { (*p).quarantined } {
        report(
            b"[R4D] module quarantined until reboot: ",
            name.bytes(),
            b"\r\n",
        );
        return LOAD_FAILED;
    }
    if find_runtime(name.bytes()).is_some() {
        report(b"[R4D] already loaded: ", name.bytes(), b"\r\n");
        return ALREADY;
    }
    report(b"[R4D] load ", name.bytes(), b" type=");
    log(type_name(unsafe { (*p).kind }));
    log(b" format=r4m\r\n");
    let registry_slot = unsafe {
        registry::r4os_rust_driver_registry_begin(
            name.data.as_ptr(),
            name.len,
            (*p).kind,
            1,
            source,
        )
    };
    if registry_slot == usize::MAX {
        log(b"[R4D] driver registry metadata/identity exhausted\r\n");
        return LOAD_FAILED;
    }
    let owner = u32::try_from(registry_slot + 1).expect("driver owner");
    let Some(init) = (unsafe { (*p).init }) else {
        set_state(registry_slot, 6);
        return INVALID_FILE;
    };
    let Some(shutdown) = (unsafe { (*p).shutdown }) else {
        log(b"[R4D] missing DriverShutdown before init\r\n");
        set_state(registry_slot, 6);
        return INVALID_FILE;
    };
    if !admission::r4os_rust_driver_api_enter(owner, u64::MAX) {
        log(b"[R4D] owner context busy\r\n");
        set_state(registry_slot, 6);
        return LOAD_FAILED;
    }
    if !admission::r4os_rust_driver_api_bind(owner, unsafe { (*p).module_slot }) {
        set_state(registry_slot, 6);
        admission::r4os_rust_driver_api_leave();
        return LOAD_FAILED;
    }
    assert!(admission::configure_callbacks(owner, unsafe {
        (*p).parallel_callbacks
    }));
    let result = unsafe { init(crate::driver::api_admission::table()) };
    admission::r4os_rust_driver_api_leave();
    if result != 0 {
        log(b"[R4D] init failed code=");
        signed(result);
        log(b"\r\n");
        let ok = failed_cleanup(shutdown, owner, name.bytes(), p, registry_slot);
        unsafe { (*p).quarantined = !ok };
        set_state(registry_slot, if ok { 6 } else { 7 });
        return INIT_FAILED;
    }
    set_state(registry_slot, 2);
    set_state(registry_slot, 3);
    let Some(runtime_slot) = free_runtime() else {
        log(b"[R4D] driver runtime metadata memory exhausted\r\n");
        let ok = failed_cleanup(shutdown, owner, name.bytes(), p, registry_slot);
        unsafe { (*p).quarantined = !ok };
        set_state(registry_slot, if ok { 6 } else { 7 });
        return LOAD_FAILED;
    };
    let d = Runtime {
        used: true,
        generation: generation(),
        kind: unsafe { (*p).kind },
        registry_slot,
        module_slot: unsafe { (*p).module_slot },
        owner,
        shutdown: Some(shutdown),
        name: Name::stored(name.bytes()),
        ..Runtime::EMPTY
    };
    unsafe { ptr::write(rt(runtime_slot), d) };
    report(b"[R4D] runtime load ", name.bytes(), b" [OK]\r\n");
    LOADED
}
fn quarantine(p: *mut Runtime) {
    unsafe { (*p).quarantined = true };
    set_state(unsafe { (*p).registry_slot }, 7);
    let name = unsafe { (*p).name };
    if let Some(slot) = find_module(name.bytes()) {
        unsafe { (*md(slot)).quarantined = true };
    }
    report(
        b"[R4D] runtime driver quarantined until reboot name=",
        name.bytes(),
        b" owner=",
    );
    decimal(u64::from(unsafe { (*p).owner }));
    log(b"\r\n");
}
fn unload(name: &[u8]) -> bool {
    if name.is_empty() {
        log(b"Usage: UNLOAD driver\r\n");
        return false;
    }
    let Some(slot) = find_runtime(name) else {
        report(b"[R4D] driver not loaded: ", name, b"\r\n");
        return false;
    };
    let p = rt(slot);
    let display_name = unsafe { (*p).name };
    if unsafe { (*p).quarantined } {
        report(
            b"[R4D] driver quarantined until reboot: ",
            display_name.bytes(),
            b"\r\n",
        );
        return false;
    }
    let g = unsafe { (*p).generation };
    let owner = unsafe { (*p).owner };
    let registry_slot = unsafe { (*p).registry_slot };
    if !admission::r4os_rust_driver_api_enter(owner, u64::MAX) {
        log(b"[R4D] owner context busy\r\n");
        return false;
    }
    if !identity(p, g, owner, registry_slot) {
        admission::r4os_rust_driver_api_leave();
        log(b"[R4D] unload target changed while waiting for owner\r\n");
        return false;
    }
    if unsafe { (*p).quarantined } {
        admission::r4os_rust_driver_api_leave();
        log(b"[R4D] driver quarantined while waiting for owner\r\n");
        return false;
    }
    set_state(registry_slot, 4);
    let Some(shutdown) = (unsafe { (*p).shutdown }) else {
        log(b"[R4D] missing shutdown entry\r\n");
        quarantine(p);
        admission::r4os_rust_driver_api_leave();
        return false;
    };
    let mut out = MaybeUninit::<cleanup::Token>::uninit();
    if !unsafe { cleanup::r4os_rust_driver_cleanup_prepare(owner, out.as_mut_ptr()) } {
        log(b"[R4D] unload vetoed: owner resources still busy\r\n");
        set_state(registry_slot, 3);
        admission::r4os_rust_driver_api_leave();
        return false;
    }
    let mut token = unsafe { out.assume_init() };
    let input = unsafe { (*p).kind == 3 };
    if !input {
        unsafe { cleanup::r4os_rust_driver_cleanup_begin(&mut token) }
    }
    let result = unsafe { shutdown() };
    if input {
        unsafe { cleanup::r4os_rust_driver_cleanup_begin(&mut token) }
    }
    if result != 0 {
        unsafe { cleanup::r4os_rust_driver_cleanup_quarantine(&mut token) };
        quarantine(p);
        admission::r4os_rust_driver_api_leave();
        log(b"[R4D] shutdown failed code=");
        signed(result);
        log(b"\r\n");
        return false;
    }
    if !unsafe { cleanup::r4os_rust_driver_cleanup_commit(&mut token) } {
        quarantine(p);
        admission::r4os_rust_driver_api_leave();
        log(b"[R4D] unload cleanup failed\r\n");
        return false;
    }
    set_state(registry_slot, 5);
    report(b"[R4D] unload ", display_name.bytes(), b" [OK]\r\n");
    unsafe { ptr::write(p, Runtime::EMPTY) };
    admission::r4os_rust_driver_api_leave();
    true
}
fn transition_kind(kind: u16) -> bool {
    matches!(kind, 1 | 3 | 5 | 6)
}
fn transition() -> bool {
    let Some(_life) = Life::enter(handoff(), true) else {
        log(b"[R4D] system-transition lifecycle timeout\r\n");
        return false;
    };
    if unsafe { (*DATA.0.get()).transition } {
        return false;
    }
    unsafe { (*DATA.0.get()).transition = true };
    let mut success = true;
    let mut stopped = 0;
    for i in 0..module_capacity() {
        let d = unsafe { ptr::read(md(i)) };
        if d.used
            && d.quarantined
            && (transition_kind(d.kind) || crate::driver::entropy::has_module(d.module_slot))
        {
            report(
                b"[R4D] system-transition quarantined module blocks warm reset name=",
                d.name.bytes(),
                b"\r\n",
            );
            success = false;
        }
    }
    for index in (0..runtime_capacity()).rev() {
        let p = rt(index);
        if unsafe {
            !(*p).used
                || !(transition_kind((*p).kind) || crate::driver::entropy::has_owner((*p).owner))
        } {
            continue;
        }
        if unsafe { (*p).quarantined } {
            success = false;
            continue;
        }
        let g = unsafe { (*p).generation };
        let owner = unsafe { (*p).owner };
        let registry_slot = unsafe { (*p).registry_slot };
        if !admission::r4os_rust_driver_api_enter(owner, handoff()) {
            log(b"[R4D] system-transition owner timeout owner=");
            decimal(u64::from(owner));
            log(b" registry=");
            decimal(registry_slot as u64);
            log(b"\r\n");
            success = false;
            continue;
        }
        if !identity(p, g, owner, registry_slot) || unsafe { (*p).quarantined } {
            success = false;
            admission::r4os_rust_driver_api_leave();
            continue;
        }
        let display = unsafe { (*p).kind == 6 };
        let audio = unsafe { (*p).kind == 1 } || crate::driver::entropy::has_owner(owner);
        if display && !cleanup::r4os_rust_driver_system_transition(owner, 0) {
            quarantine(p);
            log(b"[R4D] system-transition display admission failed\r\n");
            success = false;
            admission::r4os_rust_driver_api_leave();
            continue;
        }
        let mut audio_token = MaybeUninit::<cleanup::Token>::uninit();
        if audio {
            if !unsafe {
                cleanup::r4os_rust_driver_cleanup_prepare(owner, audio_token.as_mut_ptr())
            } {
                quarantine(p);
                log(b"[R4D] system-transition audio admission failed\r\n");
                success = false;
                admission::r4os_rust_driver_api_leave();
                continue;
            }
            unsafe { cleanup::r4os_rust_driver_cleanup_begin(audio_token.as_mut_ptr()) };
        }
        set_state(registry_slot, 4);
        let name = unsafe { (*p).name };
        report(
            b"[R4D] system-transition shutdown name=",
            name.bytes(),
            b" owner=",
        );
        decimal(u64::from(unsafe { (*p).owner }));
        log(b"\r\n");
        let result = unsafe { (*p).shutdown.map_or(-1, |shutdown| shutdown()) };
        let admission_closed = !display || cleanup::r4os_rust_driver_system_transition(owner, 1);
        let audio_quiesced = if audio {
            if result != 0 {
                unsafe { cleanup::r4os_rust_driver_cleanup_quarantine(audio_token.as_mut_ptr()) };
                false
            } else {
                unsafe { cleanup::r4os_rust_driver_cleanup_commit(audio_token.as_mut_ptr()) }
            }
        } else {
            true
        };
        let quiesced = admission_closed
            && result == 0
            && audio_quiesced
            && (!display || cleanup::r4os_rust_driver_system_transition(owner, 2));
        if !quiesced {
            quarantine(p);
            let name = unsafe { (*p).name };
            report(
                b"[R4D] system-transition shutdown failed name=",
                name.bytes(),
                b" code=",
            );
            signed(result);
            if result == 0 {
                log(b" retained-resources=active");
            }
            log(b"\r\n");
            success = false;
            admission::r4os_rust_driver_api_leave();
            continue;
        }
        let name = unsafe { (*p).name };
        report(
            b"[R4D] system-transition shutdown OK name=",
            name.bytes(),
            b"\r\n",
        );
        stopped += 1;
        admission::r4os_rust_driver_api_leave();
    }
    log(b"[R4D] system-transition network/display/input/audio shutdown stopped=");
    decimal(stopped);
    log(if success {
        b" result=OK\r\n"
    } else {
        b" result=FAILED\r\n"
    });
    success
}
fn discover() {
    let Some(_life) = Life::enter(u64::MAX, false) else {
        return;
    };
    for i in 0..module_capacity() {
        unsafe { ptr::write(md(i), Module::EMPTY) };
    }
    let Some(volume) = mount() else {
        log(b"[R4D] C: FAT32 volume not mounted\r\n");
        super::performance::record_r4d_results(0);
        return;
    };
    let Some(mut req) = begin_request() else {
        return;
    };
    let result = unsafe {
        vfs::r4os_rust_vfs_run(
            0,
            &t::Args {
                volume,
                name: span(b"/R4OS/DRIVERS"),
                ..t::Args::default()
            },
            ptr::null_mut(),
        )
    };
    unsafe { request::r4os_rust_fs_request_finish(&mut req, result.status == 0) };
    if result.status != 0 {
        log(b"[R4D] /R4OS/DRIVERS not found\r\n");
        super::performance::record_r4d_results(0);
        return;
    }
    let dir = result.value;
    log(b"[R4D] scanning /R4OS/DRIVERS\r\n");
    let mut index = 0;
    let mut discovered = 0;
    let mut name = [0; 64];
    while let Some(mut req) = begin_request() {
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
        unsafe { request::r4os_rust_fs_request_finish(&mut req, true) };
        super::performance::add_r4d_scan_ticks(start);
        if result.status != 0 {
            break;
        }
        super::performance::record_r4d_scan_entry();
        let entry = unsafe { entry.assume_init() };
        let name = &name[..zlen(&name)];
        if !entry.is_dir() && extension(name) {
            super::performance::record_r4d_candidate();
            if entry.size != 0 {
                let source = file::Source {
                    volume,
                    entry,
                    drive: b'C',
                };
                if info_file(&source, name).is_some() {
                    let mut output = [0; 96];
                    let path = path(b"C:\\R4OS\\DRIVERS\\", name, &mut output);
                    let start = super::performance::now();
                    let ok = load_file(&source, name, path, false) == LOADED;
                    super::performance::add_r4d_probe_ticks(start);
                    if ok {
                        discovered += 1;
                    }
                }
            }
        }
        index += 1;
    }
    super::performance::record_r4d_results(discovered as usize);
    log(b"[R4D] modules discovered=");
    decimal(discovered);
    log(b"\r\n");
}
/// # Safety
/// Called under the existing linear boot/discovery admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4d_discover() {
    discover()
}
/// # Safety
/// Resident path through admitted synchronous file and module calls.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4d_load_path(name: t::Bytes) -> bool {
    let Some(_life) = Life::enter(u64::MAX, false) else {
        return false;
    };
    let name = unsafe { bytes(name) };
    let Ok(source) = read_file(name) else {
        return false;
    };
    load_file(&source, fallback(name), name, false) == LOADED
}
/// # Safety
/// Resident raw name through lifecycle, I/O and driver callbacks.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4d_load(name: t::Bytes) -> u8 {
    let Some(_life) = Life::enter(u64::MAX, false) else {
        return LOAD_FAILED;
    };
    let name = unsafe { bytes(name) };
    if name.is_empty() {
        log(b"Usage: LOAD driver\r\n");
        return NAME_INVALID;
    }
    if active(name) {
        return ALREADY;
    }
    let mut buffer = [0; 96];
    let Some(path) = driver_path(name, &mut buffer) else {
        log(b"[R4D] driver name too long\r\n");
        return NAME_INVALID;
    };
    let source = match read_file(path) {
        Ok(s) => s,
        Err(2) => return FILE_MISSING,
        Err(3) => return INVALID_FILE,
        Err(_) => return LOAD_FAILED,
    };
    load_file(&source, fallback(path), path, true)
}
/// # Safety
/// Preload bytes and name stay resident through all driver/loader callbacks.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4d_preload(name: t::Bytes, data: t::Bytes) -> u8 {
    let Some(_life) = Life::enter(u64::MAX, false) else {
        return LOAD_FAILED;
    };
    let name = unsafe { bytes(name) };
    let data = unsafe { bytes(data) };
    if name.is_empty() {
        return NAME_INVALID;
    }
    if active(name) {
        return ALREADY;
    }
    let mut buffer = [0; 96];
    let path = path(b"PRELOAD:\\", name, &mut buffer);
    if magic(data) {
        return preload(data, name, path);
    }
    log(if data.starts_with(b"R4D0") {
        b"[R4D] Legacy preload module format not supported\r\n"
    } else if data.len() < 64 {
        b"[R4D] bad preload file\r\n"
    } else {
        b"[R4D] bad preload R4M0 magic\r\n"
    });
    INVALID_FILE
}
/// # Safety
/// Resident name and exact caller task/unwind through shutdown acknowledgments.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4d_unload(name: t::Bytes) -> bool {
    let Some(_life) = Life::enter(u64::MAX, false) else {
        return false;
    };
    unload(unsafe { bytes(name) })
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_r4d_transition() -> bool {
    transition()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_r4d_succeeded(result: u8) -> bool {
    matches!(result, LOADED | ALREADY)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_r4d_result_name(result: u8) -> t::Bytes {
    span(match result {
        LOADED => b"loaded",
        ALREADY => b"already-active",
        NAME_INVALID => b"name-invalid",
        FILE_MISSING => b"file-missing",
        INVALID_FILE => b"invalid-file",
        INIT_FAILED => b"init-failed",
        LOAD_FAILED => b"load-failed",
        _ => unreachable!(),
    })
}
/// # Safety
/// Aligned exclusive output; copy under lifecycle, then release before caller
/// memory writes. This never reopens or reads the installed module file.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4d_info(owner: u32, out: *mut api::DriverModuleInfo) -> u8 {
    let Some(_life) = Life::enter(0, false) else {
        return 1;
    };
    for i in 0..runtime_capacity() {
        let p = rt(i);
        if unsafe { !(*p).used || (*p).owner != owner } {
            continue;
        }
        let entry = ctl::r4os_rust_mod_entry(unsafe { (*p).module_slot });
        if entry.is_null() || unsafe { (*entry).kind != 3 || (*entry).state != 2 } {
            return 0;
        }
        let name = unsafe { (*p).name };
        let mut value = api::DriverModuleInfo {
            owner,
            generation: unsafe { (*p).generation },
            module_generation: unsafe { (*entry).generation },
            flags: u32::from(unsafe { (*p).quarantined })
                | if unsafe { (*entry).version_labels.module[0] != 0 } {
                    2
                } else {
                    0
                }
                | if unsafe { (*entry).version_labels.firmware[0] != 0 } {
                    4
                } else {
                    0
                },
            module_version: unsafe { (*entry).version_labels.module },
            firmware_version: unsafe { (*entry).version_labels.firmware },
            ..api::DriverModuleInfo::default()
        };
        let n = name.len.min(value.driver_name.len() - 1);
        value.driver_name[..n].copy_from_slice(&name.data[..n]);
        unsafe { out.write(value) };
        return 2;
    }
    0
}
