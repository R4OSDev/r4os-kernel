//! Program file resolution and the complete admission/load/start transaction.
//! The existing native VFS, request gates, bounded module cache, R4M reader,
//! image/stack owners and publication paths retain their exact resource contracts.
use super::{
    async_io, entry, exit_state, failure, image,
    launch_lifetime::{self as lifetime, Loaded, Options},
    memory, payload, policy, registry, registry_state,
    registry_types::*,
    stack, thread_runtime, threads, util,
};
use crate::{
    boot::foundation,
    fs::{
        drive::Drive,
        request, vfs,
        vfs_types::{self as t, Bytes},
    },
    loader::{file, r4m},
    memory::scratch::Scratch,
    sched::task::{
        task_api, task_registry,
        task_types::{ExecutionOwnerKind, State as TaskState, Task},
    },
};
use core::{
    cell::UnsafeCell,
    ffi::c_void,
    mem::{self, MaybeUninit},
    ptr, slice,
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct File {
    pub source: file::Source,
    pub origin: [u8; 128],
    pub origin_len: u16,
}
struct Attempts(UnsafeCell<u64>);
// SAFETY: same original program launch/diagnostic admission. No shared native
// reference or additional counter exists; the legacy view is a copied value.
unsafe impl Sync for Attempts {}
static ATTEMPTS: Attempts = Attempts(UnsafeCell::new(0));
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_launch_attempts() -> u64 {
    unsafe { ATTEMPTS.0.get().read() }
}

fn report(enabled: bool, text: &[u8]) {
    entry::report_stage(enabled, text, true);
}
fn detail(text: &[u8]) {
    unsafe { program_entry_boot_detail(util::span(text)) }
}
struct FsGuard {
    guard: request::Guard,
    ok: bool,
}
impl Drop for FsGuard {
    fn drop(&mut self) {
        unsafe { request::r4os_rust_fs_request_finish(&mut self.guard, self.ok) }
    }
}
struct TaskPin(*mut Task);
impl Drop for TaskPin {
    fn drop(&mut self) {
        unsafe { task_api::r4os_rust_task_mutate(18, self.0.cast(), 0, 0, ptr::null(), 0) };
    }
}
fn kind_label(kind: u32) -> &'static [u8] {
    match kind {
        1 | 10 | 11 | 12 => b"Metadaten",
        2 | 3 | 19 | 20 => b"lesen",
        4 | 5 | 21 | 22 => b"schreiben",
        6 => b"Stream beginnen",
        7 => b"Stream schreiben",
        8 => b"Stream beenden",
        9 => b"Stream abbrechen",
        13 | 24 => b"Datei loeschen",
        14 => b"Ordner anlegen",
        15 => b"Ordner loeschen",
        16 => b"umbenennen",
        17 => b"kopieren",
        18 => b"verschieben",
        23 | 25 => b"Update schreiben",
        _ => unreachable!("Private filesystem request kind"),
    }
}
unsafe fn module_basename(instance: *const ProgramInstance) -> Option<Bytes> {
    unsafe {
        if instance.is_null() || (*instance).runtime_payload.is_null() {
            return None;
        }
        let runtime = payload::runtime(instance);
        let len = (*runtime).module_path_len as usize;
        if len == 0 || len > 128 {
            return None;
        }
        let name = util::basename(&(&(*runtime).module_path)[..len]);
        if name.is_empty() {
            None
        } else {
            Some(util::span(name))
        }
    }
}
fn blog(text: &[u8]) {
    stack::blog(text)
}
fn bdec(value: u64) {
    stack::bdec(value)
}
unsafe fn fs_wait(enabled: bool, letter: u8) {
    unsafe {
        if !enabled || !crate::display::bootscreen::is_active() {
            return;
        }
        let mut snapshot = MaybeUninit::<request::GateSnapshot>::uninit();
        if !request::r4os_rust_fs_request_gate(letter, snapshot.as_mut_ptr()) {
            report(enabled, b"FS-Sperre wartet");
            return;
        }
        let snapshot = snapshot.assume_init();
        let raw = task_api::r4os_rust_task_identity(
            0,
            snapshot.owner_task_id,
            snapshot.owner_task_generation,
            ptr::null_mut(),
            0,
        );
        if raw == 0 {
            detail(b"FS-Halter fehlt");
            blog(b"[FSGATE] desktop wait owner missing task=");
            bdec(u64::from(snapshot.owner_task_id));
            blog(b" generation=");
            bdec(snapshot.owner_task_generation);
            blog(b" kind=");
            bdec(u64::from(snapshot.kind));
            blog(b"\r\n");
            return;
        }
        let task = ptr::with_exposed_provenance_mut::<Task>(raw as usize);
        let _pin = TaskPin(task);
        let core = task_registry::core(task);
        let mut owner = Bytes {
            data: (*core).name.pointer,
            len: (*core).name.len,
        };
        let mut activity = util::span(kind_label(snapshot.kind));
        match (*core).execution_owner_kind {
            ExecutionOwnerKind::ProgramThread => {
                let thread = (*core).execution_owner_context.cast::<threads::Thread>();
                if !thread.is_null()
                    && (*thread).used
                    && (*thread).task_id == snapshot.owner_task_id
                    && (*thread).task_generation == snapshot.owner_task_generation
                    && let Some(name) = module_basename((*thread).owner_instance)
                {
                    owner = name;
                }
            }
            ExecutionOwnerKind::AsyncIo => {
                let mut view = MaybeUninit::<async_io::RequestView>::uninit();
                if async_io::r4os_rust_program_async_view(task, view.as_mut_ptr()) {
                    let view = view.assume_init();
                    if let Some(name) = module_basename(view.instance) {
                        owner = name;
                    }
                    if view.path.len != 0 && view.path.len <= 1024 {
                        let name = util::basename(util::bytes(view.path));
                        if !name.is_empty() {
                            activity = util::span(name);
                        }
                    }
                }
            }
            ExecutionOwnerKind::None | ExecutionOwnerKind::DriverThread => {}
        }
        if (*core).state == TaskState::Blocked && (*core).wait_reason.len != 0 {
            activity = Bytes {
                data: (*core).wait_reason.pointer,
                len: (*core).wait_reason.len,
            };
        }
        program_launch_service_stage(owner, activity);
        blog(b"[FSGATE] desktop wait owner_task=");
        bdec(u64::from(snapshot.owner_task_id));
        blog(b" owner_generation=");
        bdec(snapshot.owner_task_generation);
        blog(b" kind=");
        bdec(u64::from(snapshot.kind));
        blog(b" owner=");
        blog(util::bytes(owner));
        blog(b" activity=");
        blog(util::bytes(activity));
        blog(b"\r\n");
    }
}
pub(super) unsafe fn resolve(drive: *const Drive, path: Bytes, boot: bool) -> Option<File> {
    unsafe {
        let letter = (*drive).letter;
        if !file::r4os_rust_module_file_allowed(letter) {
            foundation::puts(b"Module start requires the running RAM volume\r\n");
            return None;
        }
        let mut volume = MaybeUninit::<t::Volume>::uninit();
        if !vfs::r4os_rust_vfs_mount(
            4,
            letter,
            ptr::null(),
            ptr::null(),
            volume.as_mut_ptr().cast(),
        ) {
            return None;
        }
        let volume = volume.assume_init();
        report(boot, b"FS-Sperre pruefen");
        let bound = request::BoundMount {
            reference: volume.reference,
            present: volume.has_ref,
        };
        let mut guard = MaybeUninit::<request::Guard>::uninit();
        if !request::r4os_rust_fs_request_begin(
            19,
            letter,
            0,
            0,
            true,
            &bound,
            1,
            true,
            guard.as_mut_ptr(),
        ) {
            fs_wait(boot, letter);
            if !request::r4os_rust_fs_request_begin(
                19,
                letter,
                0,
                0,
                false,
                &bound,
                1,
                true,
                guard.as_mut_ptr(),
            ) {
                return None;
            }
        }
        let mut guard = FsGuard {
            guard: guard.assume_init(),
            ok: false,
        };
        report(boot, b"Dateipfad suchen");
        let args = t::Args {
            volume,
            name: path,
            ..t::Args::default()
        };
        let mut entry = MaybeUninit::<t::Entry>::uninit();
        if vfs::r4os_rust_vfs_run(1, &args, entry.as_mut_ptr()).status != 0 {
            return None;
        }
        let entry = entry.assume_init();
        if entry.is_dir() {
            return None;
        }
        if entry.size == 0 {
            foundation::puts(b"Bad R4X file\r\n");
            return None;
        }
        guard.ok = true;
        let mut result = File {
            source: file::Source {
                volume,
                entry,
                drive: letter,
            },
            origin: [0; 128],
            origin_len: 0,
        };
        let path = util::bytes(path);
        let mut len = 0;
        if path.len() + 3 <= 128 {
            result.origin[0] = letter;
            result.origin[1] = b':';
            len = 2;
            if path.is_empty() || !matches!(path[0], b'/' | b'\\') {
                result.origin[2] = b'\\';
                len = 3;
            }
            for &byte in path {
                result.origin[len] = if byte == b'/' { b'\\' } else { byte };
                len += 1;
            }
        }
        result.origin_len = len as u16;
        Some(result)
    }
}
struct Reader {
    source: file::Source,
    size: usize,
    windows: [file::Window; 2],
    stamp: u64,
    totals: file::MetadataTotals,
}
// Fields are dropped in declaration order: publish before freeing scratch.
struct ReaderWorkspace {
    _statistics: file::MetadataPublication,
    storage: Scratch<Reader>,
}
impl ReaderWorkspace {
    fn as_mut_ptr(&self) -> *mut Reader {
        self.storage.as_mut_ptr()
    }
}
impl Reader {
    unsafe fn create(source: *const file::Source) -> Option<ReaderWorkspace> {
        let workspace = Scratch::<Self>::new()?;
        let reader = workspace.as_mut_ptr();
        unsafe {
            ptr::addr_of_mut!((*reader).totals).write(file::MetadataTotals::EMPTY);
            ptr::addr_of_mut!((*reader).size).write((*source).entry.size as usize);
            ptr::copy_nonoverlapping(source, ptr::addr_of_mut!((*reader).source), 1);
            file::r4os_rust_module_file_reset(
                ptr::addr_of_mut!((*reader).windows).cast(),
                ptr::addr_of_mut!((*reader).stamp),
            );
        }
        Some(ReaderWorkspace {
            _statistics: unsafe { file::MetadataPublication::new(ptr::addr_of!((*reader).totals)) },
            storage: workspace,
        })
    }
}
#[repr(C)]
struct ReadContext {
    reader: *mut Reader,
    name: Bytes,
    verbose: bool,
}
unsafe extern "C" fn range(
    context: *mut c_void,
    offset: usize,
    out: *mut u8,
    len: usize,
) -> file::Output {
    unsafe {
        let context = &*context.cast::<ReadContext>();
        file::r4os_rust_module_file_range(
            ptr::addr_of!((*context.reader).source),
            offset,
            out,
            len,
            context.name,
            context.verbose,
        )
    }
}
unsafe extern "C" fn cached(
    reader: *mut c_void,
    offset: usize,
    out: *mut u8,
    len: usize,
    name: Bytes,
    verbose: bool,
) -> bool {
    unsafe {
        let reader = reader.cast::<Reader>();
        let mut context = ReadContext {
            reader,
            name,
            verbose,
        };
        let io = file::Io {
            context: ptr::addr_of_mut!(context).cast(),
            read: range,
            name,
            verbose,
        };
        file::r4os_rust_module_file_cached(
            ptr::addr_of_mut!((*reader).windows).cast(),
            ptr::addr_of_mut!((*reader).stamp),
            ptr::addr_of_mut!((*reader).totals),
            (*reader).size,
            offset,
            out,
            len,
            &io,
        )
    }
}
unsafe extern "C" fn read(context: *mut c_void, offset: usize, out: *mut u8, len: usize) -> bool {
    unsafe {
        let context = &*context.cast::<ReadContext>();
        cached(
            context.reader.cast(),
            offset,
            out,
            len,
            context.name,
            context.verbose,
        )
    }
}
unsafe extern "C" fn read_error(context: *mut c_void, reason: *const u8, len: usize) {
    unsafe {
        let context = &*context.cast::<ReadContext>();
        if context.verbose {
            foundation::puts(b"[R4M] ");
            foundation::puts(slice::from_raw_parts(reason, len));
            foundation::puts(b" failed ");
            foundation::puts(util::bytes(context.name));
            foundation::puts(b"\r\n");
        }
    }
}
fn reader_io(context: &mut ReadContext) -> r4m::Io {
    r4m::Io {
        context: ptr::from_mut(context).cast(),
        read,
        log: read_error,
        file_size: unsafe { (*context.reader).size },
    }
}
fn reject(text: &[u8], verbose: bool) -> bool {
    if verbose {
        foundation::puts(text);
    }
    false
}
fn validate_header(header: r4m::Header, verbose: bool) -> bool {
    if header.flags & !7 != 0 {
        return reject(b"Unsupported R4M0 flags\r\n", verbose);
    }
    if (header.flags & 7).count_ones() > 1 {
        return reject(b"Conflicting R4M0 app class flags\r\n", verbose);
    }
    if header.entry_count == 0 {
        return reject(b"Invalid R4M0 entry count\r\n", verbose);
    }
    true
}
unsafe fn header(reader: *mut Reader, name: &[u8], verbose: bool) -> Option<r4m::Header> {
    unsafe {
        let mut context = ReadContext {
            reader,
            name: util::span(name),
            verbose,
        };
        let io = reader_io(&mut context);
        let limits = r4m::Limits {
            max_sections: 16,
            max_imports: 16,
            max_exports: 16,
            max_relocations: u32::MAX,
            require_sections: true,
            require_entries: true,
        };
        let mut header = MaybeUninit::uninit();
        if !r4m::r4os_rust_r4m_header(&io, 1, &limits, header.as_mut_ptr()) {
            return None;
        }
        let header = header.assume_init();
        if validate_header(header, verbose) {
            Some(header)
        } else {
            None
        }
    }
}
unsafe fn exports(reader: *mut Reader, header: r4m::Header, verbose: bool) -> Option<(bool, bool)> {
    unsafe {
        let mut v1 = false;
        let mut v2 = false;
        for index in 0..header.export_count {
            let mut context = ReadContext {
                reader,
                name: util::span(b"r4x-export-table"),
                verbose,
            };
            let io = reader_io(&mut context);
            let mut record = MaybeUninit::<r4m::ExportRecord>::uninit();
            if !r4m::r4os_rust_r4m_record(
                &io,
                &header,
                index as usize,
                3,
                record.as_mut_ptr().cast(),
            ) {
                return None;
            }
            let record = record.assume_init();
            let mut name = [0; 64];
            context.name = util::span(b"r4x-export-name");
            let io = reader_io(&mut context);
            let span =
                r4m::r4os_rust_r4m_zstring(&io, record.name_offset, name.as_mut_ptr(), name.len());
            if !span.present {
                return None;
            }
            if &name[span.offset..span.offset + span.len] != b"R4XStart" {
                continue;
            }
            if record.version >= 1 {
                v1 = true;
            }
            if record.version >= 2 {
                v2 = true;
            }
        }
        Some((v1, v2))
    }
}
unsafe fn metadata(
    reader: *mut Reader,
    header: r4m::Header,
    class: u8,
    export: (bool, bool),
    verbose: bool,
) -> bool {
    unsafe {
        let mut data = [0u8; 2048];
        let mut context = ReadContext {
            reader,
            name: util::span(b"r4x-metadata-probe"),
            verbose,
        };
        let io = reader_io(&mut context);
        let span = r4m::r4os_rust_r4m_metadata(&io, &header, data.as_mut_ptr(), data.len());
        if !span.present {
            return false;
        }
        let value = &data[span.offset..span.offset + span.len];
        if !policy::r4os_rust_program_start_accepts(
            value.as_ptr(),
            value.len(),
            export.0 && !export.1,
            header.export_count,
        ) {
            return false;
        }
        let mut memory = MaybeUninit::<memory::Contract>::uninit();
        memory::r4os_rust_program_memory_contract(util::span(value), class, memory.as_mut_ptr())
            && policy::r4os_rust_program_parallel(value.as_ptr(), value.len()) != u8::MAX
            && policy::r4os_rust_program_desktop_host(value.as_ptr(), value.len()) != u8::MAX
    }
}
pub(super) unsafe fn classify(file: &File, policy: u32) -> Option<u8> {
    unsafe {
        let reader = Reader::create(ptr::addr_of!(file.source))?;
        let header = header(reader.as_mut_ptr(), b"r4x-classify-header", false)?;
        let class = util::class(policy, header.flags);
        let export = exports(reader.as_mut_ptr(), header, false)?;
        if metadata(reader.as_mut_ptr(), header, class, export, false) {
            Some(class)
        } else {
            None
        }
    }
}
struct Spawn(*mut threads::Thread);
impl Drop for Spawn {
    fn drop(&mut self) {
        unsafe { thread_runtime::r4os_rust_program_thread_leave_spawn(self.0) }
    }
}
struct Reservation {
    value: ProgramInstanceReservation,
    active: bool,
}
impl Drop for Reservation {
    fn drop(&mut self) {
        if self.active {
            unsafe { registry::r4os_rust_program_registry_reservation(0, &self.value) };
        }
    }
}
fn log_loader(
    trace: entry::Trace,
    start: u64,
    before: file::Stats,
    fs_before: request::Summary,
    loaded: &image::Loaded,
) {
    let now = entry::r4os_rust_program_entry_now();
    let mut after = MaybeUninit::<file::Stats>::uninit();
    let mut fs_after = MaybeUninit::<request::Summary>::uninit();
    unsafe {
        file::r4os_rust_module_file_stats(after.as_mut_ptr());
        request::r4os_rust_fs_request_summary(fs_after.as_mut_ptr());
    }
    let after = unsafe { after.assume_init() };
    let fs_after = unsafe { fs_after.assume_init() };
    foundation::puts(b"[R4BASIC-LAUNCH] id=");
    foundation::puts(unsafe { util::bytes(trace.id) });
    foundation::puts(b" mode=");
    foundation::puts(unsafe { util::bytes(trace.mode) });
    foundation::puts(b" phase=loader-complete ns=");
    foundation::dec(now);
    for (label, value) in [
        (b" duration_ns=".as_slice(), now.saturating_sub(start)),
        (
            b" range_reads=",
            after.range_reads.saturating_sub(before.range_reads),
        ),
        (
            b" fs_requests=",
            fs_after.requests.saturating_sub(fs_before.requests),
        ),
        (
            b" gate_waits=",
            fs_after
                .lock_contention_waits
                .saturating_sub(fs_before.lock_contention_waits),
        ),
        (
            b" fs_ticks=",
            fs_after.total_ticks.saturating_sub(fs_before.total_ticks),
        ),
        (b" sections=", u64::from(loaded.section_count)),
        (b" imports=", u64::from(loaded.import_count)),
        (b" relocations=", u64::from(loaded.relocation_count)),
    ] {
        foundation::puts(label);
        foundation::dec(value);
    }
    foundation::puts(b"\r\n");
}
#[allow(clippy::too_many_arguments)]
pub(super) unsafe fn run(
    file: &File,
    mode: u8,
    policy: u32,
    args: Bytes,
    working_drive: *mut Drive,
    host: u8,
    options: &Options,
) -> u8 {
    unsafe {
        let counter = ATTEMPTS.0.get();
        counter.write(counter.read().wrapping_add(1));
        let mut trace = MaybeUninit::<entry::Trace>::uninit();
        let trace = if entry::r4os_rust_program_entry_trace(args, trace.as_mut_ptr()) {
            Some(trace.assume_init())
        } else {
            None
        };
        let mut start = 0;
        let mut module_before = MaybeUninit::<file::Stats>::uninit();
        let mut fs_before = MaybeUninit::<request::Summary>::uninit();
        if let Some(trace) = trace {
            start = entry::r4os_rust_program_entry_now();
            file::r4os_rust_module_file_stats(module_before.as_mut_ptr());
            request::r4os_rust_fs_request_summary(fs_before.as_mut_ptr());
            entry::r4os_rust_program_entry_trace_phase(trace, util::span(b"admission"), start);
        }
        report(options.report_boot, b"API vorbereiten");
        super::runtime_tables::configure(0);
        lifetime::error(options, 0);
        let spawn = threads::current();
        if !spawn.is_null() {
            if (*spawn).spawn_transaction_depth == u32::MAX {
                lifetime::error(options, -10);
                return 2;
            }
            (*spawn).spawn_transaction_depth += 1;
        }
        let _spawn = Spawn(spawn);
        if !options.out_handle.is_null() {
            options
                .out_handle
                .write(crate::contract::ProgramProcessHandle::default());
        }
        if mode != 2 {
            (*exit_state::state()).last_display_used = false;
        }
        report(options.report_boot, b"Slot reservieren");
        let mut reservation = MaybeUninit::uninit();
        match registry::r4os_rust_program_registry_reserve(reservation.as_mut_ptr()) {
            0 => {}
            failure => {
                foundation::puts(b"Program registry allocation failed\r\n");
                lifetime::error(
                    options,
                    match failure {
                        1 => -8,
                        2 => -11,
                        _ => unreachable!(),
                    },
                );
                return 2;
            }
        }
        let mut reservation = Reservation {
            value: reservation.assume_init(),
            active: true,
        };
        report(options.report_boot, b"Completion anlegen");
        let completion = registry::CompletionRequest {
            reservation: &reservation.value,
            owner: options.owner,
            override_present: options.owner_handle_present,
            owner_handle: options.owner_handle,
            legacy_id: options.legacy_id,
            retain_output: options.retain_output,
        };
        if failure::r4os_rust_program_failure_control(3, 1)
            || !registry::r4os_rust_program_registry_completion_allocate(&completion)
        {
            lifetime::error(options, -8);
            return 2;
        }
        report(options.report_boot, b"Header laden");
        let Some(reader) = Reader::create(ptr::addr_of!(file.source)) else {
            lifetime::error(options, -8);
            return 2;
        };
        let Some(header) = header(reader.as_mut_ptr(), b"r4x-image-header", true) else {
            lifetime::error(options, -9);
            return 2;
        };
        if threads::r4os_rust_program_thread_scalar(1) != 0 {
            lifetime::error(options, -10);
            return 2;
        }
        let class = util::class(policy, header.flags);
        if host != 0 && class != 0 {
            foundation::puts(b"Console host requires a console program\r\n");
            lifetime::error(options, -1);
            return 2;
        }
        report(options.report_boot, b"Image laden");
        let bridge = image::Reader {
            context: reader.as_mut_ptr().cast(),
            read: cached,
        };
        let request = image::Request {
            reader: ptr::addr_of!(bridge).cast_mut().cast(),
            source: ptr::addr_of!(file.source).cast(),
            header,
            file_size: file.source.entry.size as usize,
            owner_id: reservation.value.id,
            app_class: class,
        };
        let mut loaded = MaybeUninit::<image::Loaded>::uninit();
        if !image::r4os_rust_program_image_load(&request, loaded.as_mut_ptr()) {
            lifetime::error(options, -9);
            return 2;
        }
        let loaded = loaded.assume_init();
        // No load callback retains the reader after image_load returns.
        // Release its cache and kill pin before publishing/running the child.
        drop(reader);
        if let Some(trace) = trace {
            log_loader(
                trace,
                start,
                module_before.assume_init(),
                fs_before.assume_init(),
                &loaded,
            );
        }
        let loaded = Loaded {
            value: loaded,
            origin: file.origin,
            origin_len: file.origin_len,
        };
        if threads::r4os_rust_program_thread_scalar(1) != 0 {
            stack::r4os_rust_program_image_release(loaded.value.image.range_id);
            lifetime::error(options, -10);
            return 2;
        }
        if failure::r4os_rust_program_failure_control(3, 3) {
            stack::r4os_rust_program_image_release(loaded.value.image.range_id);
            lifetime::error(options, -9);
            return 2;
        }
        report(options.report_boot, b"Stack anlegen");
        let mut stack = MaybeUninit::<stack::Stack>::uninit();
        let mut stack_trace = MaybeUninit::<entry::Trace>::uninit();
        let measured = entry::r4os_rust_program_entry_trace(args, stack_trace.as_mut_ptr());
        if !stack::r4os_rust_program_stack_allocate(
            reservation.value.id,
            loaded.value.memory.limits.stack_reserve,
            loaded.value.memory.limits.stack_initial_commit,
            loaded.value.memory.profile,
            measured,
            stack.as_mut_ptr(),
        ) {
            foundation::puts(b"Program stack allocation failed\r\n");
            stack::r4os_rust_program_image_release(loaded.value.image.range_id);
            lifetime::error(options, -8);
            return 2;
        }
        let stack = stack.assume_init();
        if threads::r4os_rust_program_thread_scalar(1) != 0 {
            stack::r4os_rust_program_resources_free(&loaded.value.image, &stack);
            lifetime::error(options, -10);
            return 2;
        }
        if failure::r4os_rust_program_failure_control(3, 4) {
            stack::r4os_rust_program_resources_free(&loaded.value.image, &stack);
            lifetime::error(options, -8);
            return 2;
        }
        report(options.report_boot, b"Startobjekte anlegen");
        lifetime::r4os_rust_program_launch_mode(
            mode,
            &reservation.value,
            &mut reservation.active,
            &loaded,
            &stack,
            class,
            args,
            working_drive,
            host,
            options,
        )
    }
}
pub(super) fn options() -> Options {
    Options {
        owner_handle: crate::contract::ProgramProcessHandle::default(),
        out_handle: ptr::null_mut(),
        error_out: ptr::null_mut(),
        owner: false,
        owner_handle_present: false,
        legacy_id: false,
        retain_output: false,
        report_boot: false,
        forced_cpu: 0,
        forced_cpu_present: false,
        reserved: 0,
    }
}
/// # Safety
/// Borrowed exact mounted Drive and path bytes; exclusive copied file output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_launch_resolve(
    drive: *const Drive,
    path: Bytes,
    boot: bool,
    out: *mut File,
) -> bool {
    unsafe {
        let Some(file) = resolve(drive, path, boot) else {
            return false;
        };
        out.write(file);
        true
    }
}
/// # Safety
/// Exact retained file/options/caller namespace through the yielding transaction.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn r4os_rust_program_launch_file(
    file: *const File,
    mode: u8,
    policy: u32,
    args: Bytes,
    drive: *mut Drive,
    host: u8,
    options: *const Options,
) -> u8 {
    unsafe { run(&*file, mode, policy, args, drive, host, &*options) }
}
/// # Safety
/// Borrowed file and exclusive optional scalar classification output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_launch_classify(
    file: *const File,
    policy: u32,
    out: *mut u8,
) -> bool {
    unsafe {
        if let Some(class) = classify(&*file, policy) {
            out.write(class);
            true
        } else {
            false
        }
    }
}
/// # Safety
/// Original mounted drives/path/argument spans and optional caller-owned handle.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn r4os_rust_program_launch_path(
    mode: u8,
    drive: *const Drive,
    path: Bytes,
    args: Bytes,
    working: *mut Drive,
    host: u8,
    handle: *mut crate::contract::ProgramProcessHandle,
) -> i32 {
    unsafe {
        let boot = mode == 1 && crate::display::bootscreen::is_active();
        if mode == 1 {
            report(boot, b"Pfad pruefen");
        }
        let Some(file) = resolve(drive, path, boot) else {
            return if mode <= 1 { 1 } else { -1 };
        };
        if mode == 3 {
            return classify(&file, 0).map_or(-2, |class| i32::from(class) + 1);
        }
        let mut options = options();
        match mode {
            0 => {
                options.owner = true;
                options.owner_handle_present = true;
            }
            1 => {
                report(boot, b"Start vorbereiten");
                options.report_boot = boot;
            }
            2 => {
                options.owner = true;
                options.owner_handle_present = true;
                options.legacy_id = true;
                options.out_handle = handle;
            }
            _ => unreachable!(),
        }
        let result = run(&file, mode, 0, args, working, host, &options);
        if mode == 2 {
            match result {
                0 => i32::try_from((*handle).instance_id).expect("Legacy program ID"),
                1 => -1,
                2 => -2,
                _ => unreachable!(),
            }
        } else {
            result as i32
        }
    }
}
fn origin_matches(origin: &[u8], letter: u8, path: &[u8]) -> bool {
    if origin.len() != path.len() + 2
        || origin.len() < 3
        || origin[1] != b':'
        || !origin[0].eq_ignore_ascii_case(&letter)
    {
        return false;
    }
    origin[2..].iter().zip(path).all(|(&a, &b)| {
        let a = if a == b'/' {
            b'\\'
        } else {
            a.to_ascii_uppercase()
        };
        let b = if b == b'/' {
            b'\\'
        } else {
            b.to_ascii_uppercase()
        };
        a == b
    })
}
/// # Safety
/// Resident bounded origin/path borrowed for this value-only comparison.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_launch_origin(
    origin: Bytes,
    letter: u8,
    path: Bytes,
) -> bool {
    unsafe { origin_matches(util::bytes(origin), letter, util::bytes(path)) }
}
/// # Safety
/// Original registry admission is acquired internally; path remains resident.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_launch_running(letter: u8, path: Bytes) -> i32 {
    unsafe {
        if !registry::lock() {
            return -2;
        }
        struct Unlock;
        impl Drop for Unlock {
            fn drop(&mut self) {
                registry::unlock();
            }
        }
        let _guard = Unlock;
        let mut chunk = (*registry_state::state()).head;
        while !chunk.is_null() {
            for i in 0..16 {
                let slot = ptr::addr_of!((*chunk).slots[i]);
                if !matches!((*slot).state, 2..=4) {
                    continue;
                }
                let runtime = (*slot)
                    .instance
                    .runtime_payload
                    .cast::<super::payload_types::Runtime>();
                if runtime.is_null() {
                    continue;
                }
                let len = (*runtime).module_path_len as usize;
                if len > 128 {
                    continue;
                }
                if origin_matches(&(&(*runtime).module_path)[..len], letter, util::bytes(path)) {
                    return 1;
                }
            }
            chunk = (*chunk).next;
        }
        0
    }
}
/// # Safety
/// Optional stable instance and exclusive copied string span output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_launch_basename(
    instance: *const ProgramInstance,
    out: *mut Bytes,
) -> bool {
    unsafe {
        if let Some(value) = module_basename(instance) {
            out.write(value);
            true
        } else {
            false
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_launch_kind(kind: u32) -> Bytes {
    util::span(kind_label(kind))
}
/// # Safety
/// Same retained task/mount observation lifetime as original boot wait report.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_launch_fs_wait(enabled: bool, letter: u8) {
    unsafe { fs_wait(enabled, letter) }
}
const _: () = {
    assert!(mem::size_of::<File>() == 1072);
    assert!(mem::align_of::<File>() == 8);
    assert!(mem::offset_of!(File, source) == 0);
    assert!(mem::offset_of!(File, origin) == 936);
    assert!(mem::offset_of!(File, origin_len) == 1064);
};

unsafe fn program_launch_service_stage(name: Bytes, activity: Bytes) {
    unsafe {
        crate::display::bootscreen::service_stage(name.data, name.len, activity.data, activity.len)
    }
}

unsafe fn program_entry_boot_detail(value: Bytes) {
    unsafe { crate::display::bootscreen::set_detail(value.data, value.len) }
}
