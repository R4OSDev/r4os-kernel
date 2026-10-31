//! Original main/shell/worker entry, R4XStart context and explicit launch traces.
//! Execution pins retain the exact instance and stack across module code. No
//! Rust reference into payload/GUI/registry state survives a yield or callback.
use super::{
    exit, exit_state, registry, registry_types::ProgramInstance, stack, thread_runtime, threads,
};
use crate::{
    arch::{callback_abort as abort, percpu},
    boot::foundation as console,
    contract::{
        self, ProgramProcessHandle as Handle, R4XStartContext as Context, R4XStartImport as Import,
    },
    fs::vfs_types::Bytes,
    platform::{monotonic, timer},
    sched::{scheduler, task::task_registry},
};
use core::{
    cell::UnsafeCell,
    mem, ptr, slice,
    sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
};
#[repr(C)]
pub struct State {
    pub entries_started: u64,
    pub attach_wait_events: u64,
    pub probe_main_mask: u64,
    pub probe_service_mask: u64,
    pub probe_thread_mask: u64,
    pub probe_bad_inheritance: u32,
    pub probe_active: bool,
}
struct Storage(UnsafeCell<State>);
// SAFETY: every later access, including diagnostic reset/read, is atomic.
// There is one backing record and no cached mirror.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    entries_started: 0,
    attach_wait_events: 0,
    probe_main_mask: 0,
    probe_service_mask: 0,
    probe_thread_mask: 0,
    probe_bad_inheritance: 0,
    probe_active: false,
}));
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_entry_state() -> *mut State {
    STATE.0.get()
}
#[repr(C)]
struct View {
    context: *mut Context,
    args: Bytes,
    imports: *const Import,
    import_count: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Trace {
    pub id: Bytes,
    pub mode: Bytes,
}
unsafe extern "C" {
    // Bounded metadata projections into owners not yet migrated in this file.

    fn r4os_call_program(entry: usize, arg: usize, stack_top: u64) -> i32;
}

unsafe fn invoke(thread: *mut threads::Thread, entry: usize, arg: usize, top: u64) -> i32 {
    unsafe {
        if thread.is_null() || !(*thread).execution_pinned || !(*thread).fault_frame.is_null() {
            return r4os_call_program(entry, arg, top);
        }
        let flags: u64;
        core::arch::asm!("pushfq", "pop {}", out(reg) flags, options(preserves_flags));
        if flags & ((1 << 9) | (1 << 10)) != 1 << 9 {
            return r4os_call_program(entry, arg, top);
        }
        let mut frame = abort::Frame::default();
        (*thread).execution_aborted = false;
        (*thread).fault_frame = ptr::addr_of_mut!(frame);
        let code = abort::r4os_rust_program_invoke(entry, arg, top, ptr::addr_of_mut!(frame));
        (*thread).fault_frame = ptr::null_mut();
        if (*thread).execution_aborted {
            // This runs on the retained kernel stack after restoring its
            // callee registers. No module epilogue/callback is resumed.
            thread_runtime::exit_thread(thread, code, Some(contract::program_exit_reason_failed));
        }
        code
    }
}
fn span(bytes: &[u8]) -> Bytes {
    Bytes {
        data: bytes.as_ptr(),
        len: bytes.len(),
    }
}
unsafe fn bytes<'a>(bytes: Bytes) -> &'a [u8] {
    if bytes.len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(bytes.data, bytes.len) }
    }
}
pub(super) fn report_stage(enabled: bool, detail: &[u8], launch: bool) {
    if !enabled || !{ crate::display::bootscreen::is_active() } {
        return;
    }
    unsafe { program_entry_boot_detail(span(detail)) };
    if launch {
        stack::blog(b"[LAUNCH] shell stage=");
        stack::blog(detail);
        stack::blog(b"\r\n");
    }
}
unsafe fn boot_detail(enabled: bool, detail: &[u8]) {
    if enabled {
        unsafe { program_entry_boot_detail(span(detail)) };
    }
}
fn cstring(bytes: &[u8]) -> usize {
    let mut len = 0;
    while len < bytes.len() && bytes[len] != 0 {
        len += 1;
    }
    len
}
unsafe extern "C" fn should_close(_context: *const Context) -> u32 {
    let instance = unsafe { super::context::current() };
    if instance.is_null() {
        1
    } else {
        u32::from(unsafe {
            AtomicBool::from_ptr(ptr::addr_of_mut!((*instance).close_requested))
                .load(Ordering::Acquire)
        })
    }
}
unsafe extern "C" fn yield_current(_context: *const Context) {
    scheduler::yield_task();
}
unsafe fn prepare(instance: *mut ProgramInstance) -> View {
    unsafe {
        let runtime = super::payload::runtime(instance);
        let process = super::payload::process(instance);
        let view = View {
            context: ptr::addr_of_mut!((*runtime).r4xstart_context),
            args: Bytes {
                data: ptr::addr_of!((*process).args).cast(),
                len: 128,
            },
            imports: ptr::addr_of!((*runtime).r4xstart_imports).cast(),
            import_count: (*runtime).r4xstart_import_count,
        };
        let mut flags =
            contract::r4xstart_flag_close_supported | contract::r4xstart_flag_yield_supported;
        if view.import_count > 0 {
            flags |= contract::r4xstart_flag_imports_valid;
        }
        let len = cstring(bytes(view.args));
        view.context.write(Context {
            magic: contract::r4xstart_magic,
            abi_major: contract::r4xstart_abi_major,
            abi_minor: contract::r4xstart_abi_minor,
            size: contract::r4xstart_context_size,
            flags,
            instance_id: u64::from((*instance).id),
            app_class: match (*instance).app_class {
                0 => 1,
                1 => 2,
                2 => 3,
                _ => unreachable!(),
            },
            reserved0: 0,
            program_path: 0,
            args: view.args.data.addr() as u64,
            args_len: len as u64,
            imports: if view.import_count > 0 {
                view.imports.addr() as u64
            } else {
                0
            },
            import_count: view.import_count,
            reserved1: 0,
            reserved_memory0: 0,
            reserved_memory1: 0,
            exit: 0,
            should_close: should_close as *const () as usize as u64,
            r#yield: yield_current as *const () as usize as u64,
            reserved_runtime: 0,
            reserved2: 0,
        });
        view
    }
}
fn length(cursor: &mut &[u8]) -> Option<usize> {
    let mut length = 0;
    let mut digits = 0;
    while digits < cursor.len() && cursor[digits].is_ascii_digit() {
        if length > 127 {
            return None;
        }
        length = length * 10 + usize::from(cursor[digits] - b'0');
        digits += 1;
    }
    if digits == 0 || digits >= cursor.len() || cursor[digits] != b':' {
        return None;
    }
    *cursor = &cursor[digits + 1..];
    Some(length)
}
fn record<'a>(remaining: &mut &'a [u8]) -> Option<(&'a [u8], &'a [u8])> {
    if remaining.is_empty() || remaining[0] != b';' {
        return None;
    }
    let mut cursor = &remaining[1..];
    let key_len = length(&mut cursor)?;
    if key_len == 0 || key_len > cursor.len() {
        return None;
    }
    let key = &cursor[..key_len];
    cursor = &cursor[key_len..];
    if cursor.is_empty() || cursor[0] != b'=' {
        return None;
    }
    cursor = &cursor[1..];
    let value_len = length(&mut cursor)?;
    if value_len > cursor.len() {
        return None;
    }
    let value = &cursor[..value_len];
    *remaining = &cursor[value_len..];
    Some((key, value))
}
fn parse(args: &[u8]) -> Option<Trace> {
    let mut remaining = args.strip_prefix(b"R4SUBSYS1")?;
    let mut id = None;
    let mut mode = b"?".as_slice();
    while !remaining.is_empty() {
        let (key, value) = record(&mut remaining)?;
        if key.eq_ignore_ascii_case(b"T") {
            if id.is_some() || value.len() != 16 || !value.iter().all(u8::is_ascii_hexdigit) {
                return None;
            }
            id = Some(value);
        } else if key.eq_ignore_ascii_case(b"M") {
            mode = value;
        }
    }
    Some(Trace {
        id: span(id?),
        mode: span(mode),
    })
}
fn now() -> u64 {
    monotonic::now_nanoseconds().unwrap_or_else(timer::event_nanoseconds)
}
unsafe fn trace_phase(trace: Trace, phase: &[u8], now: u64) {
    console::puts(b"[R4BASIC-LAUNCH] id=");
    console::puts(unsafe { bytes(trace.id) });
    console::puts(b" mode=");
    console::puts(unsafe { bytes(trace.mode) });
    console::puts(b" phase=");
    console::puts(phase);
    console::puts(b" ns=");
    console::dec(now);
    console::puts(b"\r\n");
}
unsafe fn note_parallel(instance: *const ProgramInstance, worker: bool) {
    unsafe {
        let state = STATE.0.get();
        if !AtomicBool::from_ptr(ptr::addr_of_mut!((*state).probe_active)).load(Ordering::Acquire)
            || !(*instance).parallel_execution
        {
            return;
        }
        let mask = 1u64 << percpu::current_index();
        AtomicU64::from_ptr(if worker {
            ptr::addr_of_mut!((*state).probe_thread_mask)
        } else {
            ptr::addr_of_mut!((*state).probe_main_mask)
        })
        .fetch_or(mask, Ordering::AcqRel);
        if !worker && (*instance).app_class == 2 {
            AtomicU64::from_ptr(ptr::addr_of_mut!((*state).probe_service_mask))
                .fetch_or(mask, Ordering::AcqRel);
        }
        let current = scheduler::current();
        if !current.is_null() && !(*task_registry::core(current.cast())).smp_eligible {
            AtomicU32::from_ptr(ptr::addr_of_mut!((*state).probe_bad_inheritance))
                .fetch_add(1, Ordering::AcqRel);
        }
    }
}
unsafe fn pending(instance: *const ProgramInstance) -> bool {
    unsafe {
        AtomicBool::from_ptr(ptr::addr_of_mut!(
            (*super::payload::gui(instance)).start_attach_pending
        ))
        .load(Ordering::Acquire)
    }
}
unsafe fn call(instance: *mut ProgramInstance) -> i32 {
    unsafe {
        let Some(entry) = thread_runtime::normalize(instance, (*instance).entry.addr()) else {
            return -1;
        };
        (*instance).entry = ptr::with_exposed_provenance(entry);
        let view = prepare(instance);
        let args = bytes(view.args);
        if let Some(trace) = parse(&args[..cstring(args)]) {
            trace_phase(trace, b"r4xstart", now());
        }
        AtomicU64::from_ptr(ptr::addr_of_mut!((*STATE.0.get()).entries_started))
            .fetch_add(1, Ordering::Relaxed);
        let thread = threads::current();
        let thread = if !thread.is_null() && (*thread).owner_instance == instance {
            thread
        } else {
            ptr::null_mut()
        };
        let exit_code = invoke(thread, entry, view.context.addr(), (*instance).stack_top);
        if let Some(mut stack) = stack::from_instance(instance) {
            stack::r4os_rust_program_stack_measure(&mut stack);
            stack::r4os_rust_program_stack_to_instance(instance, &stack);
            if stack.serial_telemetry {
                stack::r4os_rust_program_stack_log(instance, &stack, 0);
            }
        }
        exit_code
    }
}
unsafe fn main(shell: bool) -> ! {
    unsafe {
        let thread = threads::current();
        if thread.is_null() {
            threads::exit_task();
        }
        if shell {
            let s = exit_state::state();
            let report = crate::display::bootscreen::is_active()
                && (*s).shell_present
                && (*s).shell_id == (*thread).instance_id
                && (*s).shell_generation == (*thread).instance_generation;
            report_stage(report, b"Shelltask gestartet", true);
            let instance = threads::pin_execution(thread);
            if instance.is_null() {
                threads::mark(thread, false, -2);
                threads::exit_task();
            }
            report_stage(report, b"Instanz gebunden", true);
            let handle = Handle {
                instance_id: (*thread).instance_id,
                generation: (*thread).instance_generation,
                ..Handle::default()
            };
            registry::r4os_rust_program_registry_retire(3, ptr::null_mut(), handle);
            threads::mark(thread, true, 0);
            report_stage(report, b"R4XStart aufrufen", true);
            let code = call(instance);
            threads::mark(thread, false, code);
            exit::r4os_rust_program_exit_mark(handle, code);
            console::puts(b"Shell program returned\r\n");
            threads::unpin_execution(thread);
            threads::exit_task();
        }
        let instance = threads::pin_execution(thread);
        if instance.is_null() {
            threads::mark(thread, false, -2);
            threads::exit_task();
        }
        let handle = Handle {
            instance_id: (*thread).instance_id,
            generation: (*thread).instance_generation,
            ..Handle::default()
        };
        let s = exit_state::state();
        let report = crate::display::bootscreen::is_active()
            && (*s).foreground_present
            && (*s).foreground_id == handle.instance_id
            && (*s).foreground_generation == handle.generation;
        registry::r4os_rust_program_registry_retire(3, ptr::null_mut(), handle);
        threads::mark(thread, true, 0);
        note_parallel(instance, false);
        if (*instance).app_class == 1 && pending(instance) {
            AtomicU64::from_ptr(ptr::addr_of_mut!((*STATE.0.get()).attach_wait_events))
                .fetch_add(1, Ordering::Relaxed);
        }
        while (*instance).app_class == 1 && pending(instance) {
            if AtomicBool::from_ptr(ptr::addr_of_mut!((*instance).close_requested))
                .load(Ordering::Acquire)
            {
                break;
            }
            threads::yield_task();
        }
        let code = call(instance);
        boot_detail(report, b"SERVMAN: Programmende");
        threads::mark(thread, false, code);
        boot_detail(report, b"SERVMAN: Threadende");
        exit::r4os_rust_program_exit_mark(handle, code);
        boot_detail(report, b"SERVMAN: Instanzende");
        boot_detail(report, b"SERVMAN: Freigabe");
        threads::unpin_execution(thread);
        boot_detail(report, b"SERVMAN: Freigegeben");
        threads::exit_task();
    }
}
/// # Safety
/// Exact scheduler-dispatched main/shell Task owns its execution lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_entry_main(shell: bool) -> ! {
    unsafe { main(shell) }
}
/// # Safety
/// Exact dispatched worker Task and its stable thread record remain resident
/// until the original terminal scheduler retirement; module entry owns no refs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_entry_worker() -> ! {
    unsafe {
        let thread = threads::current_ready();
        if thread.is_null() {
            threads::exit_task();
        }
        let instance = threads::pin_execution(thread);
        if instance.is_null() {
            threads::mark(thread, false, -2);
            threads::exit_task();
        }
        let Some(entry) = thread_runtime::normalize(instance, (*thread).entry) else {
            threads::mark(thread, false, -1);
            threads::unpin_execution(thread);
            threads::exit_task();
        };
        (*thread).entry = entry;
        threads::mark(thread, true, 0);
        note_parallel(instance, true);
        let observation = if (*instance).desktop_host
            && (*thread).flags & crate::contract::thread_flag_compute_worker != 0
        {
            crate::sched::worker_observation::begin(
                crate::sched::worker_observation::DESKTOP,
                (*instance).id,
                (*thread).instance_generation,
                u64::from((*thread).id),
            )
        } else {
            None
        };
        let code = invoke(thread, entry, (*thread).arg, (*thread).stack.top);
        crate::sched::worker_observation::finish(observation, code, true);
        stack::r4os_rust_program_stack_measure(ptr::addr_of_mut!((*thread).stack));
        if (*thread).stack.serial_telemetry {
            stack::r4os_rust_program_stack_log(
                instance,
                ptr::addr_of!((*thread).stack),
                (*thread).id,
            );
        }
        threads::mark(thread, false, code);
        threads::unpin_execution(thread);
        threads::exit_task();
    }
}
/// # Safety
/// Exact pinned program instance; raw entry and its module imports remain live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_entry_call(instance: *mut ProgramInstance) -> i32 {
    unsafe { call(instance) }
}
/// # Safety
/// Original exact instance/runtime/process lifetime and exclusive context field.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_entry_context(instance: *mut ProgramInstance) {
    unsafe { prepare(instance) };
}
/// # Safety
/// Resident bounded byte span. No reference or pointer escapes the length query.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_entry_cstring(input: Bytes) -> u64 {
    cstring(unsafe { bytes(input) }) as u64
}
/// # Safety
/// Output is exclusive and source resident. Parsed views borrow that source;
/// malformed trace leaves output unchanged, including duplicate/nonhex IDs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_entry_trace(input: Bytes, out: *mut Trace) -> bool {
    let Some(trace) = parse(unsafe { bytes(input) }) else {
        return false;
    };
    unsafe { out.write(trace) };
    true
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_entry_now() -> u64 {
    now()
}
/// # Safety
/// Trace ID/mode and phase spans remain resident through this explicit log call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_entry_trace_phase(trace: Trace, phase: Bytes, now: u64) {
    unsafe { trace_phase(trace, bytes(phase), now) };
}
/// # Safety
/// Resident detail and boot/UI caller lifetime; launch adds original bootlog.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_entry_report(
    enabled: bool,
    detail: Bytes,
    launch: bool,
) {
    report_stage(enabled, unsafe { bytes(detail) }, launch);
}
const _: () = {
    assert!(mem::size_of::<State>() == 48);
    assert!(mem::align_of::<State>() == 8);
    assert!(mem::offset_of!(State, entries_started) == 0);
    assert!(mem::offset_of!(State, attach_wait_events) == 8);
    assert!(mem::offset_of!(State, probe_main_mask) == 16);
    assert!(mem::offset_of!(State, probe_service_mask) == 24);
    assert!(mem::offset_of!(State, probe_thread_mask) == 32);
    assert!(mem::offset_of!(State, probe_bad_inheritance) == 40);
    assert!(mem::offset_of!(State, probe_active) == 44);
    assert!(mem::size_of::<View>() == 40);
    assert!(mem::offset_of!(View, args) == 8);
    assert!(mem::offset_of!(View, imports) == 24);
    assert!(mem::offset_of!(View, import_count) == 32);
    assert!(mem::size_of::<Trace>() == 32);
    assert!(mem::offset_of!(Trace, mode) == 16);
};

unsafe fn program_entry_boot_detail(detail: Bytes) {
    unsafe { crate::display::bootscreen::set_detail(detail.data, detail.len) }
}
