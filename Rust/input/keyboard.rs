//! Sole keyboard decoder/queues. Short IRQ-safe metadata never spans polling
//! callbacks or scheduler work. The wait predicate sees an atomic projection;
//! publication precedes the Runtime-owned sequence increment and wake.
use super::{
    codepoint_queue as cp, key_layout, metadata::MetadataLock, physical_key as physical,
    text_events,
};
use crate::{
    arch::{io, pic},
    platform::timer,
    sched::{sync_objects, sync_types::WaitQueue, task::task_types::WaitResult},
    service::desktop_events,
};
use core::{
    cell::UnsafeCell,
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};
pub const IRQ: u8 = 1;
pub type InputHook = unsafe extern "C" fn() -> u8;
pub type PollHook = unsafe extern "C" fn();
struct State {
    queue: cp::Queue,
    physical: physical::Queue,
    alt_down: bool,
    altgr_down: bool,
    modifier_stale_resets: u64,
    extended: bool,
    layout: u32,
    input_hook: Option<InputHook>,
    poll_hook: Option<PollHook>,
    irq_count: u64,
    scancode_count: u64,
    decoded_count: u64,
    unmapped_character_count: u64,
    layout_fallback_count: u64,
}
struct Storage(UnsafeCell<State>);
// SAFETY: all metadata accesses hold LOCK; no borrow spans a callback/wake.
unsafe impl Sync for Storage {}
static CURRENT: Storage = Storage(UnsafeCell::new(State {
    queue: cp::Queue::EMPTY,
    physical: physical::Queue::EMPTY,
    alt_down: false,
    altgr_down: false,
    modifier_stale_resets: 0,
    extended: false,
    layout: 0,
    input_hook: None,
    poll_hook: None,
    irq_count: 0,
    scancode_count: 0,
    decoded_count: 0,
    unmapped_character_count: 0,
    layout_fallback_count: 0,
}));
static LOCK: MetadataLock = MetadataLock::new();
static READY: AtomicBool = AtomicBool::new(false);
struct Activity {
    queue: UnsafeCell<WaitQueue>,
    sequence: UnsafeCell<u64>,
}
// SAFETY: exact enrollment/sequence operations belong to scheduler Runtime.
unsafe impl Sync for Activity {}
static ACTIVITY: Activity = Activity {
    queue: UnsafeCell::new(WaitQueue::EMPTY),
    sequence: UnsafeCell::new(0),
};
fn state<T>(f: impl FnOnce(&mut State) -> T) -> T {
    let _guard = LOCK.acquire();
    let s = unsafe { &mut *CURRENT.0.get() };
    let out = f(s);
    READY.store(s.queue.pending(), Ordering::Release);
    out
}
#[derive(Default)]
struct Wake {
    input: bool,
    desktop: bool,
}
fn publish(w: Wake) {
    if w.input {
        signal_activity()
    }
    if w.desktop {
        desktop_events::r4os_rust_desktop_signal();
    }
}
pub fn signal_activity() {
    unsafe {
        sync_objects::queue_bump_sequence(ACTIVITY.queue.get(), ACTIVITY.sequence.get());
    }
}
pub fn generation() -> u64 {
    unsafe { sync_objects::queue_read_sequence(ACTIVITY.queue.get(), ACTIVITY.sequence.get()) }
}
impl State {
    fn shift_down(&self) -> bool {
        self.physical.held[0xe1] || self.physical.held[0xe5]
    }
    fn ctrl_down(&self) -> bool {
        self.physical.held[0xe0] || self.physical.held[0xe4]
    }
    fn modifiers(&self) -> u8 {
        u8::from(self.shift_down())
            | (u8::from(self.ctrl_down()) << 1)
            | (u8::from(self.alt_down) << 2)
            | (u8::from(self.altgr_down) << 3)
    }
    fn reset_modifiers(&mut self) {
        self.alt_down = false;
        self.altgr_down = false;
        self.extended = false;
    }
    fn push(&mut self, codepoint: u32, wake: &mut Wake) {
        let Some(word) = text_events::native_word(codepoint) else {
            return;
        };
        if !self.queue.try_push(word) {
            return;
        }
        self.decoded_count = self.decoded_count.wrapping_add(1);
        wake.input = true;
        wake.desktop = true;
    }
    fn handle(&mut self, scan: u8) -> Wake {
        let mut w = Wake::default();
        if scan == 0xe0 {
            self.extended = true;
            return w;
        }
        let resets = self.physical.overflow_resets;
        if let Some(usage) = physical::set1_usage(scan & 0x7f, self.extended)
            && self
                .physical
                .transition(usage, scan & 0x80 == 0, timer::tick_count())
        {
            w.desktop = true;
        }
        if self.physical.overflow_resets != resets {
            // The physical queue dropped this scan while publishing Reset.
            // Keep the text/status projection in that same reset state.
            self.reset_modifiers();
            return w;
        }
        if self.extended {
            self.extended = false;
            match scan {
                0x38 => {
                    self.altgr_down = true;
                    return w;
                }
                0xb8 => {
                    self.altgr_down = false;
                    return w;
                }
                0x1d | 0x9d => return w,
                _ => {}
            }
            if scan & 0x80 != 0 {
                return w;
            }
            let ch = match scan {
                0x48 => Some(0x80),
                0x50 => Some(0x81),
                0x4b => Some(0x88),
                0x4d => Some(0x89),
                0x47 => Some(0x8a),
                0x4f => Some(0x8b),
                0x49 => Some(0x8d),
                0x51 => Some(0x8e),
                0x53 => Some(0x7f),
                0x5b | 0x5c => Some(0x8c),
                _ => None,
            };
            if let Some(ch) = ch {
                self.push(ch, &mut w)
            }
            return w;
        }
        match scan {
            0x2a | 0x36 | 0xaa | 0xb6 | 0x1d | 0x9d => return w,
            0x38 => {
                self.alt_down = true;
                return w;
            }
            0xb8 => {
                self.alt_down = false;
                return w;
            }
            _ => {}
        }
        if scan & 0x80 != 0 {
            return w;
        }
        let special = if scan == 0x01 && self.ctrl_down() {
            Some(0x83)
        } else if scan == 0x3d {
            Some(0x82)
        } else if scan == 0x0f && self.alt_down {
            Some(if self.shift_down() { 0x87 } else { 0x85 })
        } else if scan == 0x3e && self.alt_down {
            Some(0x86)
        } else if scan == 0x0f && self.shift_down() {
            Some(0x84)
        } else {
            None
        };
        if let Some(ch) = special {
            self.push(ch, &mut w);
            return w;
        }
        if self.ctrl_down()
            && let Some(ch) = key_layout::ctrl_char(self.layout, scan)
        {
            self.push(u32::from(ch), &mut w);
            return w;
        }
        let mapped = key_layout::translate(self.layout, scan, self.shift_down(), self.altgr_down);
        if mapped.has_character() {
            self.push(mapped.codepoint, &mut w)
        } else if mapped.ascii != 0 {
            self.push(u32::from(mapped.ascii), &mut w)
        }
        w
    }
}
fn drain_output() {
    for _ in 0..256 {
        if unsafe { io::inb(0x64) } & 1 == 0 {
            break;
        }
        let _ = unsafe { io::inb(0x60) };
    }
}
pub fn init() {
    unsafe {
        sync_objects::queue_reopen(ACTIVITY.queue.get());
    }
    state(|s| {
        s.physical.clear();
        s.reset_modifiers();
    });
    drain_output();
    pic::unmask(IRQ);
}
pub fn disable() {
    pic::mask(IRQ);
    state(|s| {
        s.queue.clear();
        s.physical.clear();
        s.reset_modifiers();
    });
    unsafe {
        sync_objects::queue_close_cancelled(ACTIVITY.queue.get());
    }
    drain_output();
}
pub fn note_irq() {
    state(|s| s.irq_count = s.irq_count.wrapping_add(1));
}
pub fn controller_byte(scan: u8) {
    let w = state(|s| {
        s.scancode_count = s.scancode_count.wrapping_add(1);
        s.handle(scan)
    });
    publish(w);
}
pub fn read_codepoint() -> Option<u32> {
    read_codepoint_limited(false)
}
pub(crate) fn read_codepoint_limited(byte: bool) -> Option<u32> {
    let poll = state(|s| s.poll_hook);
    if let Some(poll) = poll {
        unsafe { poll() }
    }
    let hook = state(|s| s.input_hook);
    if let Some(hook) = hook {
        let value = unsafe { hook() };
        if value != 0 {
            return Some(u32::from(value));
        }
    }
    state(|s| text_events::take_legacy(&mut s.queue, byte))
}
pub(crate) fn read_event_word() -> Option<u32> {
    let poll = state(|s| s.poll_hook);
    if let Some(poll) = poll {
        unsafe { poll() }
    }
    let hook = state(|s| s.input_hook);
    if let Some(hook) = hook {
        let value = unsafe { hook() };
        if value != 0 {
            return text_events::native_word(u32::from(value));
        }
    }
    state(|s| s.queue.pop())
}
pub fn pending() -> bool {
    READY.load(Ordering::Acquire)
}
unsafe extern "C" fn still_needed(context: *mut c_void) -> bool {
    let last = unsafe { context.cast::<u64>().read() };
    (unsafe { ACTIVITY.sequence.get().read() }) == last && !pending()
}
pub fn wait(last: u64, timeout: u64) -> (WaitResult, u64) {
    let mut context = last;
    let result = unsafe {
        sync_objects::queue_wait_unless(
            ACTIVITY.queue.get(),
            timeout,
            crate::sched::sync_types::Text::literal(b"keyboard-input"),
            still_needed,
            ptr::addr_of_mut!(context).cast(),
        )
    };
    (result, generation())
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Snapshot {
    pub irq_count: u64,
    pub scancode_count: u64,
    pub decoded_count: u64,
    pub unmapped_character_count: u64,
    pub layout_fallback_count: u64,
    pub push_attempt_count: u64,
    pub dropped_count: u64,
    pub queue_capacity: u32,
    pub queue_pending: u32,
    pub queue_high_water: u32,
    pub modifiers: u8,
    pub modifier_stale_resets: u64,
    pub pending: bool,
    pub layout: u32,
    pub layout_name: key_layout::Text,
    pub layout_display: key_layout::Text,
}
pub fn snapshot() -> Snapshot {
    state(|s| {
        let q = s.queue.snapshot();
        let d = key_layout::definition(s.layout);
        Snapshot {
            irq_count: s.irq_count,
            scancode_count: s.scancode_count,
            decoded_count: s.decoded_count,
            unmapped_character_count: s.unmapped_character_count,
            layout_fallback_count: s.layout_fallback_count,
            push_attempt_count: q.push_attempts,
            dropped_count: q.dropped,
            queue_capacity: cp::CAPACITY,
            queue_pending: q.pending,
            queue_high_water: q.high_water,
            modifiers: s.modifiers(),
            modifier_stale_resets: s.modifier_stale_resets,
            pending: s.queue.pending(),
            layout: s.layout,
            layout_name: d.name,
            layout_display: d.display,
        }
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_keyboard_init() {
    init();
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_keyboard_disable() {
    disable();
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_keyboard_irq() {
    note_irq();
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_keyboard_byte(value: u8) {
    controller_byte(value);
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller retains readable text or an aligned exclusive output, as appropriate. Wait outputs remain live through scheduling; no runtime/metadata lock spans the call.
pub unsafe extern "C" fn r4os_rust_input_keyboard_read(out: *mut u32) -> bool {
    let Some(value) = read_codepoint() else {
        return false;
    };
    unsafe { out.write(value) }
    true
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller retains readable text or an aligned exclusive output, as appropriate. Wait outputs remain live through scheduling; no runtime/metadata lock spans the call.
pub unsafe extern "C" fn r4os_rust_input_keyboard_physical(out: *mut physical::Event) -> bool {
    let Some(value) = state(|s| s.physical.pop()) else {
        return false;
    };
    unsafe { out.write(value) }
    true
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller retains readable text or an aligned exclusive output, as appropriate. Wait outputs remain live through scheduling; no runtime/metadata lock spans the call.
pub unsafe extern "C" fn r4os_rust_input_keyboard_physical_stats(out: *mut physical::Snapshot) {
    let value = state(|s| s.physical.snapshot());
    unsafe { out.write(value) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_keyboard_pending() -> bool {
    pending()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_keyboard_generation() -> u64 {
    generation()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_keyboard_signal() {
    signal_activity();
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller retains readable text or an aligned exclusive output, as appropriate. Wait outputs remain live through scheduling; no runtime/metadata lock spans the call.
pub unsafe extern "C" fn r4os_rust_input_keyboard_wait(
    last: u64,
    timeout: u64,
    out: *mut u64,
) -> u8 {
    let (result, next) = wait(last, timeout);
    unsafe { out.write(next) }
    result as u8
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_keyboard_free() -> u32 {
    state(|s| s.queue.free_count())
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_keyboard_accept(count: u32) -> bool {
    state(|s| s.queue.can_accept(count))
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller retains readable text or an aligned exclusive output, as appropriate. Wait outputs remain live through scheduling; no runtime/metadata lock spans the call.
pub unsafe extern "C" fn r4os_rust_input_keyboard_stats(out: *mut Snapshot) {
    let value = snapshot();
    unsafe { out.write(value) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_keyboard_layout(id: u32) {
    let _ = key_layout::definition(id);
    state(|s| s.layout = id);
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller retains readable text or an aligned exclusive output, as appropriate. Wait outputs remain live through scheduling; no runtime/metadata lock spans the call.
pub unsafe extern "C" fn r4os_rust_input_keyboard_layout_name(
    p: *const u8,
    l: usize,
    fallback: bool,
) -> bool {
    let bytes = if l == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(p, l) }
    };
    let parsed = key_layout::parse_name(bytes);
    state(|s| {
        if let Some(id) = parsed {
            s.layout = id;
            true
        } else {
            if fallback {
                s.layout = 0;
                s.layout_fallback_count = s.layout_fallback_count.wrapping_add(1)
            }
            false
        }
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_keyboard_layout_id() -> u32 {
    state(|s| s.layout)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_keyboard_input_hook(hook: Option<InputHook>) {
    state(|s| s.input_hook = hook);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_keyboard_poll_hook(hook: Option<PollHook>) {
    state(|s| s.poll_hook = hook);
}
const _: () = {
    assert!(core::mem::size_of::<Snapshot>() == 120);
    assert!(core::mem::align_of::<Snapshot>() == 8);
    assert!(core::mem::offset_of!(Snapshot, modifier_stale_resets) == 72);
    assert!(core::mem::offset_of!(Snapshot, layout) == 84);
    assert!(core::mem::offset_of!(Snapshot, layout_name) == 88);
    assert!(core::mem::offset_of!(Snapshot, layout_display) == 104);
};

#[unsafe(no_mangle)]
/// # Safety
/// Caller retains readable text or an aligned exclusive output, as appropriate. Wait outputs remain live through scheduling; no runtime/metadata lock spans the call.
pub unsafe extern "C" fn r4os_rust_input_keyboard_read_char(out: *mut u8) -> bool {
    let Some(value) = read_codepoint_limited(true) else {
        return false;
    };
    let Ok(value) = u8::try_from(value) else {
        return false;
    };
    unsafe { out.write(value) }
    true
}
