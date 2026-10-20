//! Console input queues, generation waits and diagnostic capture. A waiting
//! task owns its exact input lease; enrollment atomically releases the input
//! mutex and checks the same generation/close predicate before parking.
use super::{
    console::{self, Lease},
    context, payload, payload_types as t, process, registry,
    registry_types::*,
    threads,
};
use crate::{
    boot::foundation,
    contract,
    fs::vfs_types::Bytes,
    input::text_events as events,
    sched::{
        sync_locks,
        sync_types::{Text, WaitRequest},
        sync_wait,
    },
};
use core::{
    cell::UnsafeCell,
    ffi::c_void,
    mem::{self, MaybeUninit},
    ptr, slice,
    sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering},
};
#[repr(C)]
pub struct State {
    pub push_calls: AtomicU64,
    pub batch_calls: AtomicU64,
    pub bytes_attempted: AtomicU64,
    pub bytes_accepted: AtomicU64,
    pub full_events: AtomicU64,
    pub read_calls: AtomicU64,
    pub read_empty: AtomicU64,
    pub read_bytes: AtomicU64,
    pub wait_calls: AtomicU64,
    pub wait_blocks: AtomicU64,
    pub wait_immediate: AtomicU64,
    pub wait_wakes: AtomicU64,
    pub wait_timeouts: AtomicU64,
    pub wait_cancellations: AtomicU64,
    pub capture: Bytes,
    pub capture_pos: usize,
    pub capture_present: bool,
}
struct Storage(UnsafeCell<State>);
// SAFETY: original queue locks/execution admission and cooperative diagnostic
// capture ownership remain unchanged. This is the only backing state.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    push_calls: AtomicU64::new(0),
    batch_calls: AtomicU64::new(0),
    bytes_attempted: AtomicU64::new(0),
    bytes_accepted: AtomicU64::new(0),
    full_events: AtomicU64::new(0),
    read_calls: AtomicU64::new(0),
    read_empty: AtomicU64::new(0),
    read_bytes: AtomicU64::new(0),
    wait_calls: AtomicU64::new(0),
    wait_blocks: AtomicU64::new(0),
    wait_immediate: AtomicU64::new(0),
    wait_wakes: AtomicU64::new(0),
    wait_timeouts: AtomicU64::new(0),
    wait_cancellations: AtomicU64::new(0),
    capture: Bytes {
        data: ptr::null(),
        len: 0,
    },
    capture_pos: 0,
    capture_present: false,
}));
fn state() -> *mut State {
    STATE.0.get()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_console_input_state() -> *mut State {
    state()
}

macro_rules! add {
    ($field:ident,$value:expr) => {
        statistic_add(mem::offset_of!(State, $field), $value);
    };
}
fn statistic_add(offset: usize, value: u64) {
    // Only the fourteen fixed AtomicU64 counter fields precede capture.
    assert!(offset < mem::offset_of!(State, capture) && offset.is_multiple_of(8));
    unsafe {
        (*state().byte_add(offset).cast::<AtomicU64>()).fetch_add(value, Ordering::Relaxed);
    }
}
unsafe fn keyboard_generation() -> u64 {
    unsafe { program_console_keyboard(0) }
}
unsafe fn keyboard_pending() -> bool {
    unsafe { program_console_keyboard(1) != 0 }
}
unsafe fn keyboard_signal() {
    unsafe { program_console_keyboard(2) };
}
unsafe fn keyboard_read(byte: bool) -> Option<u32> {
    crate::input::keyboard::read_codepoint_limited(byte)
}
unsafe fn keyboard_word() -> Option<u32> {
    crate::input::keyboard::read_event_word()
}

unsafe fn lock(console: *mut t::Console) -> bool {
    unsafe {
        sync_locks::r4os_rust_sync_mutex(ptr::addr_of_mut!((*console).input_lock), 1, u64::MAX)
    }
}
unsafe fn unlock(console: *mut t::Console) {
    unsafe { sync_locks::r4os_rust_sync_mutex(ptr::addr_of_mut!((*console).input_lock), 2, 0) };
}
unsafe fn sequence(console: *mut t::Console, op: u8) -> u64 {
    unsafe {
        sync_wait::r4os_rust_sync_queue(
            ptr::addr_of_mut!((*console).input_wait),
            op,
            ptr::null_mut(),
            0,
            ptr::addr_of_mut!((*console).input_generation),
        )
    }
}
unsafe fn closed(instance: *mut ProgramInstance) -> bool {
    unsafe {
        AtomicBool::from_ptr(ptr::addr_of_mut!((*instance).close_requested)).load(Ordering::Acquire)
    }
}
unsafe fn done(instance: *mut ProgramInstance) -> bool {
    unsafe { AtomicBool::from_ptr(ptr::addr_of_mut!((*instance).done)).load(Ordering::Acquire) }
}
unsafe fn target(handle: contract::ProgramProcessHandle) -> *mut ProgramInstance {
    unsafe {
        if !registry::lock() {
            return ptr::null_mut();
        }
        let slot = registry::lookup_handle(handle, false);
        let result = if slot.is_null()
            || !registry::running((*slot).state)
            || done(ptr::addr_of_mut!((*slot).instance))
            || (*slot).instance.app_class != 0
        {
            ptr::null_mut()
        } else {
            ptr::addr_of_mut!((*slot).instance)
        };
        registry::unlock();
        result
    }
}
pub(super) unsafe fn current() -> *mut ProgramInstance {
    unsafe {
        if threads::compute_worker() {
            return ptr::null_mut();
        }
        let instance = context::current();
        if instance.is_null() || (*instance).app_class != 0 {
            return ptr::null_mut();
        }
        let console = payload::console(instance);
        if (*console).io_target_id != 0 {
            return target(contract::ProgramProcessHandle {
                instance_id: (*console).io_target_id,
                reserved: 0,
                generation: (*console).io_target_generation,
            });
        }
        if (*instance).role == 2 || ((*instance).role == 1 && (*console).host != 0) {
            instance
        } else {
            ptr::null_mut()
        }
    }
}
unsafe fn pop(instance: *mut ProgramInstance, legacy: bool, byte: bool) -> Result<Option<u32>, ()> {
    unsafe {
        let console = payload::console(instance);
        if !lock(console) {
            return Err(());
        }
        if (*console).input_head == (*console).input_tail {
            unlock(console);
            return Ok(None);
        }
        let word = (*console).input_queue[(*console).input_tail];
        let value = if legacy {
            let Some(value) = events::legacy_value(word) else {
                unlock(console);
                return Err(());
            };
            if byte && value > 255 {
                unlock(console);
                return Err(());
            }
            value
        } else {
            word
        };
        let cost = events::byte_cost(word).expect("console event storage");
        assert!(
            (*console).input_pending_bytes >= cost,
            "console input byte accounting"
        );
        (*console).input_pending_bytes -= cost;
        (*console).input_queue[(*console).input_tail] = 0;
        (*console).input_tail = ((*console).input_tail + 1) % 4096;
        console::pending_locked(console);
        unlock(console);
        Ok(Some(value))
    }
}
unsafe fn push_words(instance: *mut ProgramInstance, words: &[u32]) -> usize {
    unsafe {
        let console = payload::console(instance);
        let attempted: u64 = words
            .iter()
            .map(|&w| u64::from(events::byte_cost(w).expect("validated input word")))
            .sum();
        add!(bytes_attempted, attempted);
        if !lock(console) {
            return 0;
        }
        let mut accepted = 0;
        let mut bytes: u32 = 0;
        while accepted < words.len() {
            let next = ((*console).input_head + 1) % 4096;
            let word = words[accepted];
            if next == (*console).input_tail
                || !events::admits((*console).input_pending_bytes, word)
            {
                break;
            }
            let cost = events::byte_cost(word).expect("validated input word");
            (*console).input_queue[(*console).input_head] = word;
            (*console).input_head = next;
            (*console).input_pending_bytes += cost;
            bytes += cost;
            accepted += 1;
        }
        if accepted != words.len() {
            add!(full_events, 1);
        }
        if accepted != 0 {
            add!(bytes_accepted, u64::from(bytes));
            (*console).state.stdin_bytes = (*console).state.stdin_bytes.wrapping_add(bytes);
            console::pending_locked(console);
            (*console).input_high_water = (*console)
                .input_high_water
                .max((*console).state.stdin_pending);
            sequence(console, 8);
        }
        unlock(console);
        if accepted != 0 && console::raw_presenter(instance) {
            keyboard_signal();
        }
        accepted
    }
}
unsafe fn push(instance: *mut ProgramInstance, data: &[u8], key: bool) -> usize {
    // Existing byte APIs preserve prefix counts/formats. Only the expressly
    // named key API identifies native logical controls; plain bytes are data.
    let mut accepted = 0;
    for chunk in data.chunks(64) {
        let mut words = [0u32; 64];
        for (out, &byte) in words.iter_mut().zip(chunk) {
            *out = if key {
                events::native_word(u32::from(byte)).expect("legacy key byte")
            } else {
                events::byte_word(byte)
            };
        }
        let count = unsafe { push_words(instance, &words[..chunk.len()]) };
        accepted += count;
        if count != chunk.len() {
            break;
        }
    }
    accepted
}

struct Selected {
    lease: Option<Lease>,
    keyboard: bool,
}
unsafe fn selected() -> Result<Selected, i32> {
    unsafe {
        let source = context::current();
        if source.is_null() || (*source).app_class != 0 {
            return Ok(Selected {
                lease: None,
                keyboard: true,
            });
        }
        let source_console = payload::console(source);
        let redirected = (*source_console).io_target_id != 0;
        let handle = if redirected {
            Some(contract::ProgramProcessHandle {
                instance_id: (*source_console).io_target_id,
                reserved: 0,
                generation: (*source_console).io_target_generation,
            })
        } else if (*source).role == 2 || ((*source).role == 1 && (*source_console).host != 0) {
            context::handle()
        } else {
            None
        };
        let Some(handle) = handle else {
            return Ok(Selected {
                lease: None,
                keyboard: true,
            });
        };
        let lease = Lease::handle(handle).ok_or(contract::console_input_wait_error_closed)?;
        let instance = lease.0.instance;
        let gui = (*instance).gui_payload.cast::<t::Gui>();
        let window = if gui.is_null() {
            -1
        } else {
            AtomicI32::from_ptr(ptr::addr_of_mut!((*gui).window_id)).load(Ordering::Acquire)
        };
        let keyboard = console::raw_presenter(instance)
            || (!redirected && window < 0 && (*payload::console(instance)).host == 0);
        Ok(Selected {
            lease: Some(lease),
            keyboard,
        })
    }
}
unsafe fn read_from(selection: &Selected, legacy: bool, byte: bool) -> Option<u32> {
    unsafe {
        if (*state()).capture_present {
            if (*state()).capture_pos >= (*state()).capture.len {
                return None;
            }
            let value = (*state()).capture.data.add((*state()).capture_pos).read();
            (*state()).capture_pos += 1;
            return Some(if legacy {
                u32::from(value)
            } else {
                events::byte_word(value)
            });
        }
        if let Some(lease) = &selection.lease {
            match pop(lease.0.instance, legacy, byte) {
                Ok(Some(value)) => return Some(value),
                Err(()) => return None, // Retain a blocked scalar; never bypass it.
                Ok(None) => {}
            }
        }
        if !selection.keyboard {
            return None;
        }
        if legacy {
            keyboard_read(byte)
        } else {
            keyboard_word()
        }
    }
}
unsafe fn read_codepoint(byte: bool) -> Option<u32> {
    if threads::compute_worker() {
        return None;
    }
    let selection = unsafe { selected() }.ok()?;
    unsafe { read_from(&selection, true, byte) }
}

#[repr(C)]
struct WaitContext {
    console: *mut t::Console,
    source: *mut ProgramInstance,
    last: u64,
}
unsafe extern "C" fn still_needed(raw: *mut c_void) -> bool {
    unsafe {
        let context = raw.cast::<WaitContext>();
        let console = (*context).console;
        (*console).input_generation == (*context).last
            && (*console).input_head == (*console).input_tail
            && !closed((*context).source)
    }
}
unsafe extern "C" fn release_lock(raw: *mut c_void) {
    unsafe {
        unlock(raw.cast());
    }
}
unsafe fn wait_result(result: u32) -> i32 {
    match result {
        1 => {
            add!(wait_wakes, 1);
            contract::console_input_wait_ready
        }
        2 => {
            add!(wait_timeouts, 1);
            contract::console_input_wait_timeout
        }
        3 | 4 => {
            add!(wait_cancellations, 1);
            contract::console_input_wait_error_closed
        }
        0 | 5 => contract::console_input_wait_error_failed,
        _ => unreachable!(),
    }
}
struct InputLease(*mut threads::Thread);
impl Drop for InputLease {
    fn drop(&mut self) {
        unsafe { threads::release_input(self.0) };
    }
}
unsafe fn wait(last: u64, timeout: u64, out: *mut u64) -> i32 {
    unsafe {
        if threads::compute_worker() {
            return contract::console_input_wait_error_invalid;
        }
        if out.is_null() {
            return contract::console_input_wait_error_invalid;
        }
        out.write(last);
        add!(wait_calls, 1);
        let source = context::current();
        if source.is_null() || (*source).app_class != 0 {
            return contract::console_input_wait_error_invalid;
        }
        let source_console = payload::console(source);
        let thread = threads::current();
        if thread.is_null() {
            return contract::console_input_wait_error_invalid;
        }
        let mut input = ptr::null_mut();
        if (*source_console).io_target_id != 0 {
            let handle = contract::ProgramProcessHandle {
                instance_id: (*source_console).io_target_id,
                reserved: 0,
                generation: (*source_console).io_target_generation,
            };
            let mut lease = MaybeUninit::uninit();
            if !threads::pin_input(thread, handle, lease.as_mut_ptr()) {
                return contract::console_input_wait_error_closed;
            }
            input = lease.assume_init().instance;
        } else if (*source).role == 2 || ((*source).role == 1 && (*source_console).host != 0) {
            let Some(handle) = context::handle() else {
                return contract::console_input_wait_error_invalid;
            };
            let mut lease = MaybeUninit::uninit();
            if !threads::pin_input(thread, handle, lease.as_mut_ptr()) {
                return contract::console_input_wait_error_closed;
            }
            input = lease.assume_init().instance;
        }
        let _lease = InputLease(thread);
        if !input.is_null() {
            let console = payload::console(input);
            if console::raw_presenter(input) {
                let generation = keyboard_generation();
                out.write(generation);
                if closed(source) {
                    return contract::console_input_wait_error_closed;
                }
                if !lock(console) {
                    return contract::console_input_wait_error_failed;
                }
                let queued = (*console).input_head != (*console).input_tail;
                unlock(console);
                if queued || keyboard_pending() || generation != last {
                    add!(wait_immediate, 1);
                    return contract::console_input_wait_ready;
                }
                if timeout == 0 {
                    return contract::console_input_wait_timeout;
                }
                add!(wait_blocks, 1);
                return wait_result(program_console_keyboard_wait(last, timeout, out));
            }
            if !lock(console) {
                return contract::console_input_wait_error_failed;
            }
            let generation = (*console).input_generation;
            if closed(source) {
                out.write(generation);
                unlock(console);
                add!(wait_cancellations, 1);
                return contract::console_input_wait_error_closed;
            }
            if (*console).input_head != (*console).input_tail || generation != last {
                out.write(generation);
                unlock(console);
                add!(wait_immediate, 1);
                return contract::console_input_wait_ready;
            }
            if timeout == 0 {
                out.write(generation);
                unlock(console);
                add!(wait_timeouts, 1);
                return contract::console_input_wait_timeout;
            }
            let mut context = WaitContext {
                console,
                source,
                last,
            };
            add!(wait_blocks, 1);
            let mut request = WaitRequest::predicate(
                timeout,
                Text::literal(b"console-input"),
                still_needed,
                ptr::addr_of_mut!(context).cast(),
            );
            request.release = Some(release_lock);
            request.release_context = console.cast();
            let result =
                sync_wait::r4os_rust_sync_wait(ptr::addr_of_mut!((*console).input_wait), &request);
            out.write(sequence(console, 9));
            return wait_result(result);
        }
        let generation = keyboard_generation();
        if keyboard_pending() || generation != last {
            out.write(generation);
            add!(wait_immediate, 1);
            return contract::console_input_wait_ready;
        }
        if timeout == 0 {
            out.write(generation);
            add!(wait_timeouts, 1);
            return contract::console_input_wait_timeout;
        }
        add!(wait_blocks, 1);
        wait_result(program_console_keyboard_wait(last, timeout, out))
    }
}
unsafe fn signal_payload(console: *mut t::Console) {
    unsafe {
        if !lock(console) {
            return;
        }
        sequence(console, 8);
        unlock(console);
    }
}
pub(super) unsafe fn signal(instance: *mut ProgramInstance) {
    unsafe {
        if (*instance).app_class != 0 {
            return;
        }
        let console = payload::console(instance);
        if (*console).io_target_id != 0 {
            let handle = contract::ProgramProcessHandle {
                instance_id: (*console).io_target_id,
                reserved: 0,
                generation: (*console).io_target_generation,
            };
            if let Some(target) = Lease::handle(handle) {
                signal_payload(payload::console(target.0.instance));
                if console::raw_presenter(target.0.instance) {
                    keyboard_signal();
                }
            }
            return;
        }
        if (*instance).role == 2 || ((*instance).role == 1 && (*console).host != 0) {
            signal_payload(console);
            if console::raw_presenter(instance) {
                keyboard_signal();
            }
        } else {
            keyboard_signal();
        }
    }
}
/// # Safety
/// Readable input span. The caller retains the original diagnostic/input owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_console_input_capture(data: Bytes, present: bool) {
    unsafe {
        (*state()).capture = if present {
            data
        } else {
            Bytes {
                data: ptr::null(),
                len: 0,
            }
        };
        (*state()).capture_present = present;
        (*state()).capture_pos = 0;
    }
}
/// # Safety
/// Original task input context; successful reads initialize the exclusive result.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_console_input_read(out: *mut u32, byte: bool) -> bool {
    let Some(_task) = crate::sched::task_context::Admission::acquire(false) else {
        return false;
    };
    unsafe {
        let Some(value) = read_codepoint(byte) else {
            return false;
        };
        if byte && value > 255 {
            return false;
        }
        out.write(value);
        true
    }
}
/// # Safety
/// Exact instance/input admission and readable data span.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_console_input_push(
    instance: *mut ProgramInstance,
    data: Bytes,
) -> usize {
    unsafe {
        push(
            instance,
            if data.len == 0 {
                &[]
            } else {
                slice::from_raw_parts(data.data, data.len)
            },
            false,
        )
    }
}
/// # Safety
/// Valid one-byte input or readable bulk span; lookup pins its exact owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_console_input_api_push(
    id: u32,
    data: *const u8,
    length: u32,
    key: bool,
) -> i32 {
    let Some(_task) = crate::sched::task_context::Admission::acquire(false) else {
        return -1;
    };
    unsafe {
        add!(push_calls, 1);
        if !key {
            add!(batch_calls, 1);
            if data.is_null() {
                return -5;
            }
        }
        process::r4os_rust_program_process_signal();
        let Some(lease) = Lease::id(id) else {
            return -1;
        };
        let instance = lease.0.instance;
        if done(instance) {
            return -2;
        }
        if (*instance).app_class != 0 {
            return -3;
        }
        let accepted = push(
            instance,
            if length == 0 {
                &[]
            } else {
                slice::from_raw_parts(data, length as usize)
            },
            key,
        );
        if key {
            if accepted == 1 { 0 } else { -4 }
        } else {
            i32::try_from(accepted).expect("Console pushed bytes")
        }
    }
}
/// # Safety
/// Writable output span of capacity bytes and original current input context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_console_input_api_read(
    out: *mut u8,
    capacity: u32,
) -> i32 {
    let Some(_task) = crate::sched::task_context::Admission::acquire(false) else {
        return 0;
    };
    unsafe {
        if threads::compute_worker() {
            return 0;
        }
        add!(read_calls, 1);
        if capacity == 0 {
            return 0;
        }
        let mut count = 0;
        while count < capacity as usize {
            let Some(value) = read_codepoint(true) else {
                break;
            };
            if value > 255 {
                break;
            }
            out.add(count).write(value as u8);
            count += 1;
        }
        if count == 0 {
            add!(read_empty, 1);
        } else {
            add!(read_bytes, count as u64);
        }
        i32::try_from(count).expect("Console read bytes")
    }
}
/// # Safety
/// Resident generation output and original current-thread lifetime across wait.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_console_input_wait(
    last: u64,
    timeout: u64,
    out: *mut u64,
) -> i32 {
    unsafe { wait(last, timeout, out) }
}
/// # Safety
/// Exact retained console payload.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_console_input_signal_payload(console: *mut t::Console) {
    unsafe {
        signal_payload(console);
    }
}
/// # Safety
/// Exact retained source instance.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_console_input_signal(instance: *mut ProgramInstance) {
    unsafe {
        signal(instance);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_console_input_signal_handle(
    handle: contract::ProgramProcessHandle,
) {
    unsafe {
        let mut lease = MaybeUninit::uninit();
        if !registry::r4os_rust_program_registry_pin(handle, true, lease.as_mut_ptr()) {
            return;
        }
        let lease = Lease(lease.assume_init());
        signal(lease.0.instance);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_console_input_current() -> *mut ProgramInstance {
    unsafe { current() }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_console_input_target(
    id: u32,
    handle: contract::ProgramProcessHandle,
    by_id: bool,
) -> *mut ProgramInstance {
    unsafe {
        let handle = if by_id {
            let mut out = MaybeUninit::uninit();
            if !registry::r4os_rust_program_registry_handle(3, ptr::null(), id, out.as_mut_ptr()) {
                return ptr::null_mut();
            }
            out.assume_init()
        } else {
            handle
        };
        target(handle)
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_console_input_inherited(class: u8) -> u32 {
    unsafe {
        if class != 0 {
            return 0;
        }
        let host = current();
        if host.is_null() { 0 } else { (*host).id }
    }
}
/// # Safety
/// Retained exact instance and exclusive successful byte output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_console_input_pop(
    instance: *mut ProgramInstance,
    out: *mut u8,
) -> bool {
    unsafe {
        let Ok(Some(value)) = pop(instance, true, true) else {
            return false;
        };
        out.write(value as u8);
        true
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_console_keyboard_hook() -> u8 {
    unsafe {
        let instance = current();
        if instance.is_null() {
            0
        } else {
            pop(instance, true, true).ok().flatten().unwrap_or(0) as u8
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_console_print_hook(ch: u8) -> bool {
    if foundation::r4os_rust_boot_redirect_active() {
        unsafe {
            console::sink(0, u32::from(ch), 0);
            console::sink(5, u32::from(ch), 0);
        }
        true
    } else {
        unsafe { console::r4os_rust_program_console_route(1, Bytes { data: &ch, len: 1 }, false) }
    }
}
const _: () = {
    assert!(mem::size_of::<State>() == 144);
    assert!(mem::offset_of!(State, capture) == 112);
    assert!(mem::offset_of!(State, capture_pos) == 128);
    assert!(mem::offset_of!(State, capture_present) == 136);
    assert!(mem::size_of::<WaitContext>() == 24);
    assert!(mem::offset_of!(WaitContext, last) == 16);
};

unsafe fn program_console_keyboard(op: u8) -> u64 {
    match op {
        0 => crate::input::keyboard::generation(),
        1 => u64::from(crate::input::keyboard::pending()),
        2 => {
            crate::input::keyboard::signal_activity();
            0
        }
        _ => panic!("Invalid keyboard operation"),
    }
}

unsafe fn program_console_keyboard_wait(last: u64, timeout: u64, out: *mut u64) -> u32 {
    let (r, next) = crate::input::keyboard::wait(last, timeout);
    unsafe {
        out.write(next);
    }
    r as u32
}

fn admitted_input_context() -> bool {
    !threads::compute_worker()
        && crate::memory::owner::fault_allowed()
        && crate::driver::irq_router::r4os_rust_irq_router_local(false) == 0
        && !unsafe { context::current() }.is_null()
}

/// # Safety
/// Exact handle and readable event batch remain owned by the admitted caller.
/// Complete resident copy/validation precedes every input-owner acquisition.
pub(crate) unsafe extern "C" fn push_events(
    handle: *const contract::ProgramProcessHandle,
    input: *const contract::ConsoleInputEvent,
    count: u32,
) -> i32 {
    if count > contract::console_input_event_batch_max
        || handle.is_null()
        || input.is_null()
        || !(handle as usize).is_multiple_of(8)
        || !(input as usize).is_multiple_of(4)
        || (input as usize)
            .checked_add(count as usize * mem::size_of::<contract::ConsoleInputEvent>())
            .is_none()
    {
        return contract::console_input_wait_error_invalid;
    }
    if !admitted_input_context() {
        return contract::console_input_wait_error_invalid;
    }
    let Some(_task) = crate::sched::task_context::Admission::acquire(true) else {
        return contract::console_input_wait_error_invalid;
    };
    crate::memory::owner::prepare_stack();
    if count == 0 {
        return 0;
    }
    let handle = unsafe { handle.read() };
    let mut words = [0u32; 64];
    for (index, word) in words[..count as usize].iter_mut().enumerate() {
        let value = unsafe { input.add(index).read() };
        let Some(encoded) = events::pack(value) else {
            return contract::console_input_wait_error_invalid;
        };
        *word = encoded;
    }
    process::r4os_rust_program_process_signal();
    let Some(lease) = (unsafe { Lease::handle(handle) }) else {
        return contract::console_input_wait_error_closed;
    };
    let instance = lease.0.instance;
    if unsafe { done(instance) || closed(instance) } {
        return contract::console_input_wait_error_closed;
    }
    if unsafe { (*instance).app_class != 0 } {
        return contract::console_input_wait_error_invalid;
    }
    unsafe { push_words(instance, &words[..count as usize]) as i32 }
}

/// # Safety
/// Writable complete-event output is retained by the exact calling task.
/// Copy only the successful prefix, after all target/input owners are closed.
pub(crate) unsafe extern "C" fn read_events(
    output: *mut contract::ConsoleInputEvent,
    capacity: u32,
) -> i32 {
    if capacity > contract::console_input_event_batch_max
        || output.is_null()
        || !(output as usize).is_multiple_of(4)
        || (output as usize)
            .checked_add(capacity as usize * mem::size_of::<contract::ConsoleInputEvent>())
            .is_none()
    {
        return contract::console_input_wait_error_invalid;
    }
    if !admitted_input_context() {
        return contract::console_input_wait_error_invalid;
    }
    let source = unsafe { context::current() };
    if unsafe { (*source).app_class != 0 } {
        return contract::console_input_wait_error_invalid;
    }
    let Some(_task) = crate::sched::task_context::Admission::acquire(true) else {
        return contract::console_input_wait_error_invalid;
    };
    crate::memory::owner::prepare_stack();
    if capacity == 0 {
        return 0;
    }
    let mut staging = [contract::ConsoleInputEvent::default(); 64];
    add!(read_calls, 1);
    let mut count = 0usize;
    let mut bytes = 0u64;
    {
        let selection = match unsafe { selected() } {
            Ok(v) => v,
            Err(e) => return e,
        };
        while count < capacity as usize {
            let Some(word) = (unsafe { read_from(&selection, false, false) }) else {
                break;
            };
            staging[count] = events::unpack(word).expect("console input event storage");
            bytes += u64::from(events::byte_cost(word).expect("console input event storage"));
            count += 1;
        }
    }
    if count == 0 {
        add!(read_empty, 1);
    } else {
        add!(read_bytes, bytes);
    }
    // Neither a target registry lease nor an input mutex spans pageable copy.
    unsafe {
        ptr::copy_nonoverlapping(staging.as_ptr(), output, count);
    }
    count as i32
}
