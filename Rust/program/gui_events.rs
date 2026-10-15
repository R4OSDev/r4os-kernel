//! Program GUI event admission and small window/text metadata. The original
//! program-state owner guards queue ordering and snapshots. Mouse moves are
//! coalesced by removal/append without reordering intervening key/button events.
use super::{
    console::Lease, context, gui_flow as flow, payload_types as t, process, process_data, registry,
    registry_state, registry_types::ProgramInstance, threads,
};
use crate::{
    contract::{GuiEvent as Event, GuiSize, GuiWindowInfo as WindowInfo},
    memory::owner::Guard,
    sched::sync_locks,
};
use core::{
    cell::UnsafeCell,
    mem, ptr,
    sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering},
};
#[repr(C)]
pub struct State {
    pub push_attempts: u64,
    pub accepted: u64,
    pub mouse_move_coalesced: u64,
    pub mouse_move_evicted: u64,
    pub rejected: u64,
}
struct Storage(UnsafeCell<State>);
// SAFETY: mutations retain the original program-state owner. Diagnostic reads
// retain their existing snapshot admission; there is no second backing copy.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    push_attempts: 0,
    accepted: 0,
    mouse_move_coalesced: 0,
    mouse_move_evicted: 0,
    rejected: 0,
}));
fn state() -> *mut State {
    STATE.0.get()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_gui_events_state() -> *mut State {
    state()
}
pub(super) unsafe fn allowed(instance: *const ProgramInstance) -> bool {
    unsafe {
        if !(*instance).parallel_execution {
            return true;
        }
        let thread = threads::current();
        !thread.is_null()
            && (*thread).owner_instance == instance.cast_mut()
            && (*thread).flags & threads::MAIN != 0
    }
}
pub(super) struct Producer {
    pub instance: *mut ProgramInstance,
    pub gui: *mut t::Gui,
    _operation: threads::Operation,
}
impl Producer {
    pub(super) unsafe fn enter() -> Option<Self> {
        unsafe {
            let instance = context::current();
            if instance.is_null() || !allowed(instance) {
                return None;
            }
            let operation = threads::Operation::enter()?;
            let gui = process_data::ensure_gui(instance);
            if gui.is_null() {
                return None;
            }
            if !sync_locks::r4os_rust_sync_guard(
                ptr::addr_of_mut!((*gui).operation_gate),
                2,
                u64::MAX,
            ) {
                return None;
            }
            Some(Self {
                instance,
                gui,
                _operation: operation,
            })
        }
    }
}
impl Drop for Producer {
    fn drop(&mut self) {
        unsafe {
            sync_locks::r4os_rust_sync_guard(ptr::addr_of_mut!((*self.gui).operation_gate), 3, 0)
        };
    }
}
pub(super) unsafe fn find_move(gui: *const t::Gui) -> Option<usize> {
    unsafe {
        let mut index = (*gui).event_head;
        while index != (*gui).event_tail {
            if (*gui).events[index].kind == 5 {
                return Some(index);
            }
            index = (index + 1) % 64;
        }
        None
    }
}
pub(super) unsafe fn remove(gui: *mut t::Gui, index: usize) {
    unsafe {
        let mut index = index;
        loop {
            let next = (index + 1) % 64;
            if next == (*gui).event_tail {
                break;
            }
            (*gui).events[index] = (*gui).events[next];
            index = next;
        }
        (*gui).event_tail = if (*gui).event_tail == 0 {
            63
        } else {
            (*gui).event_tail - 1
        };
    }
}
pub(super) unsafe fn pending(gui: *const t::Gui) -> u32 {
    let _guard = Guard::acquire(0);
    unsafe {
        u32::try_from(if (*gui).event_tail >= (*gui).event_head {
            (*gui).event_tail - (*gui).event_head
        } else {
            64 - (*gui).event_head + (*gui).event_tail
        })
        .expect("GUI queued events")
    }
}
pub(super) unsafe fn enqueue(gui: *mut t::Gui, event: *const Event) -> i32 {
    unsafe {
        let event = event.read();
        let _guard = Guard::acquire(0);
        (*state()).push_attempts = (*state()).push_attempts.wrapping_add(1);
        if event.kind == 5
            && let Some(index) = find_move(gui)
        {
            remove(gui, index);
            (*state()).mouse_move_coalesced = (*state()).mouse_move_coalesced.wrapping_add(1);
        }
        let next = ((*gui).event_tail + 1) % 64;
        if next == (*gui).event_head {
            if let Some(index) = find_move(gui) {
                remove(gui, index);
                (*state()).mouse_move_evicted = (*state()).mouse_move_evicted.wrapping_add(1);
            } else {
                (*state()).rejected = (*state()).rejected.wrapping_add(1);
                return -3;
            }
        }
        (*gui).events[(*gui).event_tail] = event;
        (*gui).event_tail = ((*gui).event_tail + 1) % 64;
        (*state()).accepted = (*state()).accepted.wrapping_add(1);
        let pending = pending(gui);
        if pending > (*gui).event_high_water {
            (*gui).event_high_water = pending;
        }
        0
    }
}
pub(super) unsafe fn text(instance: *mut ProgramInstance, input: *const u8) -> i32 {
    unsafe {
        let gui = process_data::ensure_gui(instance);
        if gui.is_null() {
            return -2;
        }
        let mut next = [0u8; 512];
        let mut len = 0;
        while len + 1 < next.len() && input.add(len).read() != 0 {
            next[len] = input.add(len).read();
            len += 1;
        }
        if flow::replace(instance, false).is_null() || flow::commit(instance) != 0 {
            return -2;
        }
        let _guard = Guard::acquire(0);
        (*gui).text = next;
        i32::try_from(len).expect("GUI text length")
    }
}
unsafe fn read_text<const N: usize>(
    gui: *const t::Gui,
    out: *mut u8,
    capacity: u32,
    title: bool,
) -> i32 {
    let guard = Guard::acquire(0);
    unsafe {
        let mut snapshot = [0u8; N];
        let source = if title {
            ptr::addr_of!((*gui).title).cast::<u8>()
        } else {
            ptr::addr_of!((*gui).text).cast::<u8>()
        };
        ptr::copy_nonoverlapping(source, snapshot.as_mut_ptr(), N);
        drop(guard);
        let mut len = 0;
        let max = (capacity - 1) as usize;
        while len < max && len < N && snapshot[len] != 0 {
            out.add(len).write(snapshot[len]);
            len += 1;
        }
        out.add(len).write(0);
        i32::try_from(len).expect("GUI string read length")
    }
}
/// # Safety
/// Retained exact program instance under its original producer admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_producer_allowed(
    instance: *const ProgramInstance,
) -> bool {
    unsafe { allowed(instance) }
}
/// # Safety
/// Exclusive complete window-info output under the original current task context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_window_info(out: *mut WindowInfo) -> i32 {
    unsafe {
        let instance = context::current();
        if instance.is_null() {
            out.write(Default::default());
            return -1;
        }
        let gui = (*instance).gui_payload.cast::<t::Gui>();
        if gui.is_null() {
            out.write(Default::default());
            return 0;
        }
        let guard = Guard::acquire(0);
        let value = (*gui).window_info;
        let hosted =
            AtomicI32::from_ptr(ptr::addr_of_mut!((*gui).window_id)).load(Ordering::Acquire) >= 0;
        drop(guard);
        out.write(value);
        i32::from(hosted)
    }
}
/// # Safety
/// Readable complete canonical window info; exact instance pin retained for write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_window_set(id: u32, info: *const WindowInfo) -> i32 {
    unsafe {
        process::r4os_rust_program_process_signal();
        let Some(lease) = Lease::id(id) else {
            return -1;
        };
        let instance = lease.0.instance;
        if AtomicBool::from_ptr(ptr::addr_of_mut!((*instance).done)).load(Ordering::Acquire) {
            return -2;
        }
        let gui = process_data::ensure_gui(instance);
        if gui.is_null() {
            return -3;
        }
        let value = info.read();
        let _guard = Guard::acquire(0);
        (*gui).window_info = value;
        AtomicI32::from_ptr(ptr::addr_of_mut!((*gui).window_id))
            .store(value.window_id, Ordering::Release);
        0
    }
}
/// # Safety
/// Exclusive complete canonical event output under current program lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_event_poll(out: *mut Event) -> i32 {
    unsafe {
        let instance = context::current();
        if instance.is_null() {
            out.write(Default::default());
            return -1;
        }
        let gui = (*instance).gui_payload.cast::<t::Gui>();
        if gui.is_null() {
            out.write(Default::default());
            return 0;
        }
        let guard = Guard::acquire(0);
        let mut event = Event::default();
        let available = (*gui).event_head != (*gui).event_tail;
        if available {
            event = (*gui).events[(*gui).event_head];
            (*gui).event_head = ((*gui).event_head + 1) % 64;
        }
        drop(guard);
        out.write(event);
        i32::from(available)
    }
}
/// # Safety
/// Readable complete canonical event; lookup retains the exact destination.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_event_push(id: u32, event: *const Event) -> i32 {
    unsafe {
        process::r4os_rust_program_process_signal();
        let Some(lease) = Lease::id(id) else {
            return -1;
        };
        let instance = lease.0.instance;
        if AtomicBool::from_ptr(ptr::addr_of_mut!((*instance).done)).load(Ordering::Acquire) {
            return -2;
        }
        let gui = process_data::ensure_gui(instance);
        if gui.is_null() {
            return -3;
        }
        enqueue(gui, event)
    }
}
/// # Safety
/// Exact GUI payload under original program-state/producer admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_event_enqueue(
    gui: *mut t::Gui,
    event: *const Event,
) -> i32 {
    unsafe { enqueue(gui, event) }
}
/// # Safety
/// Exact queue under original metadata admission. Optional index uses MAX sentinel.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_event_find(gui: *const t::Gui) -> usize {
    unsafe { find_move(gui).unwrap_or(usize::MAX) }
}
/// # Safety
/// Exact queued index under the original program-state owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_event_remove(gui: *mut t::Gui, index: usize) {
    unsafe {
        remove(gui, index);
    }
}
/// # Safety
/// Exact retained GUI metadata; function obtains original program-state admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_event_pending(gui: *const t::Gui) -> u32 {
    unsafe { pending(gui) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_gui_font_changed() {
    if !registry::lock() {
        return;
    }
    unsafe {
        let event = Event {
            kind: 7,
            ..Default::default()
        };
        let mut iterator = registry::Iterator {
            chunk: (*registry_state::state()).head,
            slot_index: 0,
            include_done: false,
        };
        loop {
            let instance = registry::r4os_rust_program_registry_iterator(&mut iterator);
            if instance.is_null() {
                break;
            }
            let gui = (*instance).gui_payload.cast::<t::Gui>();
            if gui.is_null()
                || AtomicI32::from_ptr(ptr::addr_of_mut!((*gui).window_id)).load(Ordering::Acquire)
                    < 0
            {
                continue;
            }
            enqueue(gui, &event);
            flow::revision(instance);
        }
    }
    registry::unlock();
}
/// # Safety
/// Exact producer instance and readable original NUL-terminated text.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_text(
    instance: *mut ProgramInstance,
    input: *const u8,
) -> i32 {
    unsafe { text(instance, input) }
}
/// # Safety
/// Readable NUL-terminated original text/title for op0/1; current GUI operation
/// retains exact unwind/gate admission. Op2/3 use no input pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_simple(
    op: u8,
    input: *const u8,
    w: i32,
    h: i32,
) -> i32 {
    unsafe {
        let Some(producer) = Producer::enter() else {
            return -1;
        };
        let instance = producer.instance;
        if op == 0 {
            return text(instance, input);
        }
        let gui = process_data::ensure_gui(instance);
        if gui.is_null() {
            return -2;
        }
        match op {
            1 => {
                let mut next = [0u8; 64];
                let mut len = 0;
                while len + 1 < next.len() && input.add(len).read() != 0 {
                    next[len] = input.add(len).read();
                    len += 1;
                }
                let guard = Guard::acquire(0);
                (*gui).title = next;
                drop(guard);
                flow::revision(instance);
                i32::try_from(len).expect("GUI title length")
            }
            2 => {
                let guard = Guard::acquire(0);
                (*gui).min_client_w = w.max(0);
                (*gui).min_client_h = h.max(0);
                drop(guard);
                flow::revision(instance);
                0
            }
            3 => {
                if !(*gui).building_frame.is_null() {
                    flow::commit(instance)
                } else {
                    flow::revision(instance);
                    0
                }
            }
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// Writable byte output of capacity bytes; retains an exact instance lease.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_string(
    id: u32,
    out: *mut u8,
    capacity: u32,
    title: bool,
) -> i32 {
    unsafe {
        process::r4os_rust_program_process_signal();
        let Some(lease) = Lease::id(id) else {
            return -1;
        };
        if capacity == 0 {
            return if title { -2 } else { 0 };
        }
        let gui = (*lease.0.instance).gui_payload.cast::<t::Gui>();
        if gui.is_null() {
            out.write(0);
            return 0;
        }
        if title {
            read_text::<64>(gui, out, capacity, true)
        } else {
            read_text::<512>(gui, out, capacity, false)
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_gui_revision_read(id: u32) -> u32 {
    unsafe {
        process::r4os_rust_program_process_signal();
        let Some(lease) = Lease::id(id) else {
            return 0;
        };
        let gui = (*lease.0.instance).gui_payload.cast::<t::Gui>();
        if gui.is_null() {
            0
        } else {
            AtomicU32::from_ptr(ptr::addr_of_mut!((*gui).revision)).load(Ordering::Acquire)
        }
    }
}
/// # Safety
/// Exclusive complete canonical size output; lookup retains the exact owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_min_size(id: u32, out: *mut GuiSize) -> i32 {
    unsafe {
        process::r4os_rust_program_process_signal();
        let Some(lease) = Lease::id(id) else {
            out.write(Default::default());
            return -1;
        };
        let gui = (*lease.0.instance).gui_payload.cast::<t::Gui>();
        if gui.is_null() {
            out.write(Default::default());
            return 0;
        }
        let guard = Guard::acquire(0);
        let value = GuiSize {
            w: (*gui).min_client_w,
            h: (*gui).min_client_h,
        };
        drop(guard);
        out.write(value);
        0
    }
}
const _: () = {
    assert!(mem::size_of::<State>() == 40);
    assert!(mem::offset_of!(State, push_attempts) == 0);
    assert!(mem::offset_of!(State, accepted) == 8);
    assert!(mem::offset_of!(State, mouse_move_coalesced) == 16);
    assert!(mem::offset_of!(State, mouse_move_evicted) == 24);
    assert!(mem::offset_of!(State, rejected) == 32);
};
