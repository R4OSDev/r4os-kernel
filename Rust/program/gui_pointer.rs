//! Native GUI pointer identities and control receipts. Desktop owns focus/WM
//! arbitration, coordinates and cursor policy; this owner stores no Task, VM,
//! image or GUI pointer beyond a call. Registry leases are acquired before
//! short program metadata admission, never while that owner is held.
use super::{
    context, gui_events, payload_types as t, registry, registry_types::ProgramInstance, threads,
};
use crate::{
    contract as c, input::mouse, memory::owner::Guard, sched::task_context, service::desktop_events,
};
use core::{
    cell::UnsafeCell,
    mem, ptr,
    sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering},
};
type Handle = c::ProgramProcessHandle;
type Token = c::GuiPointerCaptureHandle;
type Info = c::GuiPointerCaptureInfo;

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct Notice {
    event: c::GuiEvent,
    before: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Window {
    epoch: u64,
    host: Handle,
    host_epoch: u64,
    focused: bool,
    terminal: Info,
    notices: [Notice; 2],
    notice_count: u32,
}
impl Window {
    // SAFETY: these records contain only integers, bools and arrays; all-zero
    // means no window, receipt or notice and is a valid representation.
    pub const EMPTY: Self = unsafe { mem::zeroed() };
}
#[derive(Clone, Copy)]
struct Root {
    host: Handle,
    host_epoch: u64,
    host_capabilities: u32,
    revision: u64,
    serial: u64,
    value: Info,
}
struct Storage(UnsafeCell<Root>);
// SAFETY: every shared field is protected by existing program metadata owner0.
unsafe impl Sync for Storage {}
static ROOT: Storage = Storage(UnsafeCell::new(unsafe { mem::zeroed() }));
// Functional lifecycle filters only. Full identity is always rechecked under
// metadata; generation/PID reuse cannot turn a filter match into ownership.
static HOST_ID: AtomicU32 = AtomicU32::new(0);
static CAPTURE_ID: AtomicU32 = AtomicU32::new(0);
fn root() -> *mut Root {
    ROOT.0.get()
}
fn valid(owner: Handle) -> bool {
    registry::valid(owner)
}
fn same(a: Handle, b: Handle) -> bool {
    registry::equal(a, b)
}
fn live(state: u32) -> bool {
    state == c::gui_pointer_state_pending || state == c::gui_pointer_state_active
}
fn window_same(a: c::GuiPointerWindowIdentity, b: c::GuiPointerWindowIdentity) -> bool {
    a.reserved == 0
        && b.reserved == 0
        && same(a.owner, b.owner)
        && a.epoch != 0
        && a.epoch == b.epoch
        && a.window_id == b.window_id
}
fn token_same(a: Token, b: Token) -> bool {
    a.id != 0 && a.id == b.id && window_same(a.window, b.window)
}
unsafe fn gui(instance: *const ProgramInstance) -> *mut t::Gui {
    unsafe { (*instance).gui_payload.cast() }
}
unsafe fn bump() {
    unsafe {
        (*root()).revision = (*root()).revision.saturating_add(1);
        (*root()).value.revision = (*root()).revision;
    }
}
unsafe fn serial() -> Option<u64> {
    unsafe {
        let next = (*root()).serial.checked_add(1)?;
        (*root()).serial = next;
        Some(next)
    }
}
unsafe fn capabilities(present: bool) -> u32 {
    unsafe {
        let mut value = (*root()).host_capabilities;
        if !present {
            value &= !c::gui_pointer_capability_relative_physical;
        }
        value
    }
}
unsafe fn ordinary(gui: *const t::Gui) -> u32 {
    unsafe { (((*gui).event_tail + 64 - (*gui).event_head) % 64) as u32 }
}
unsafe fn notice(gui: *mut t::Gui, info: Info, tick: u64) {
    if gui.is_null() {
        return;
    }
    unsafe {
        let state = &mut (*gui).pointer;
        assert!(
            state.notice_count < 2,
            "capture gained/terminal notice bound"
        );
        state.notices[state.notice_count as usize] = Notice {
            event: c::GuiEvent {
                kind: c::GuiEventKind::capture_changed.0,
                window_id: info.capture.window.window_id,
                x: info.state as i32,
                y: info.reason as i32,
                key: info.capture.id as u32,
                buttons: (info.capture.id >> 32) as u32,
                modifiers: info.mode,
                tick,
            },
            before: ordinary(gui),
        };
        state.notice_count += 1;
    }
}
unsafe fn terminal(gui: *mut t::Gui, state: u32, reason: u32, tick: u64) {
    unsafe {
        if !live((*root()).value.state) {
            return;
        }
        (*root()).value.state = state;
        (*root()).value.reason = reason;
        bump();
        CAPTURE_ID.store(0, Ordering::Release);
        let info = (*root()).value;
        if !gui.is_null() {
            (*gui).pointer.terminal = info;
            notice(gui, info, tick);
        }
    }
}
unsafe fn signal(gui: *mut t::Gui) {
    if !gui.is_null() {
        unsafe {
            super::gui_activity::signal(gui);
        }
    }
    desktop_events::r4os_rust_desktop_signal();
}

/// Called under program metadata0 for removal/coalescing or ordinary pop.
pub(super) unsafe fn removed(gui: *mut t::Gui, index: usize) {
    unsafe {
        let offset = ((index + 64 - (*gui).event_head) % 64) as u32;
        for n in 0..(*gui).pointer.notice_count as usize {
            if offset < (*gui).pointer.notices[n].before {
                (*gui).pointer.notices[n].before -= 1;
            }
        }
    }
}
pub(super) unsafe fn pending(gui: *const t::Gui) -> u32 {
    unsafe { (*gui).pointer.notice_count }
}
/// A notice never displaces queued pointer/key events accepted before its
/// barrier. Acquisitions are backpressured until the previous notices drain.
pub(super) unsafe fn poll(gui: *mut t::Gui) -> Option<c::GuiEvent> {
    unsafe {
        if (*gui).pointer.notice_count == 0 || (*gui).pointer.notices[0].before != 0 {
            return None;
        }
        let event = (*gui).pointer.notices[0].event;
        (*gui).pointer.notices[0] = (*gui).pointer.notices[1];
        (*gui).pointer.notice_count -= 1;
        Some(event)
    }
}
/// Exact target is already leased; caller is copied before program metadata.
pub(super) unsafe fn associate(
    gui: *mut t::Gui,
    owner: Handle,
    old: i32,
    next: i32,
    source: Handle,
) {
    unsafe {
        let state = ptr::addr_of_mut!((*gui).pointer);
        let host_ok = valid((*root()).host) && same(source, (*root()).host);
        let changed = old != next
            || (*state).epoch == 0
            || !same((*state).host, source)
            || (*state).host_epoch != (*root()).host_epoch;
        if changed {
            if same((*root()).value.capture.window.owner, owner) && live((*root()).value.state) {
                terminal(
                    gui,
                    c::gui_pointer_state_cancelled,
                    if next < 0 {
                        c::gui_pointer_reason_window_closed
                    } else {
                        c::gui_pointer_reason_desktop_changed
                    },
                    0,
                );
            }
            (*state).epoch = if next >= 0 && host_ok {
                serial().unwrap_or(0)
            } else {
                0
            };
            (*state).host = if host_ok { source } else { Handle::default() };
            (*state).host_epoch = if host_ok { (*root()).host_epoch } else { 0 };
            (*state).focused = false;
            super::gui_input::rebind(
                gui,
                c::GuiPointerWindowIdentity {
                    owner,
                    epoch: (*state).epoch,
                    window_id: next,
                    reserved: 0,
                },
                if old != next {
                    c::gui_input_reset_reason_window_closed
                } else {
                    c::gui_input_reset_reason_desktop_changed
                },
            );
        }
        if next < 0 {
            (*state).epoch = 0;
            (*state).focused = false;
        }
    }
}
/// Called with exact leased GUI and metadata0 before publishing window flags.
pub(super) unsafe fn visibility(gui: *mut t::Gui, owner: Handle, flags: u32) {
    unsafe {
        if !same((*root()).value.capture.window.owner, owner) || !live((*root()).value.state) {
            return;
        }
        let reason = if flags & c::GuiWindowFlag::close_requested != 0 {
            c::gui_pointer_reason_window_closed
        } else if flags & c::GuiWindowFlag::minimized != 0 {
            c::gui_pointer_reason_minimized
        } else if flags & c::GuiWindowFlag::visible == 0 {
            c::gui_pointer_reason_window_closed
        } else {
            return;
        };
        terminal(gui, c::gui_pointer_state_cancelled, reason, 0);
    }
}
pub(super) unsafe fn focus(
    instance: *mut ProgramInstance,
    owner: Handle,
    event: c::GuiEvent,
    source: Handle,
) {
    let gui = unsafe { gui(instance) };
    if gui.is_null() {
        return;
    }
    if event.kind != c::GuiEventKind::focus_gained.0 && event.kind != c::GuiEventKind::focus_lost.0
    {
        return;
    }
    let guard = Guard::acquire(0);
    let changed = unsafe { focus_locked(gui, owner, event, source) };
    drop(guard);
    if changed {
        unsafe { signal(gui) };
    }
}
unsafe fn focus_locked(
    gui: *mut t::Gui,
    owner: Handle,
    event: c::GuiEvent,
    source: Handle,
) -> bool {
    unsafe {
        let before = revision();
        if !same((*gui).pointer.host, source) || (*gui).pointer.host_epoch != (*root()).host_epoch {
            return false;
        }
        (*gui).pointer.focused = event.kind == c::GuiEventKind::focus_gained.0;
        if !(*gui).pointer.focused && same((*root()).value.capture.window.owner, owner) {
            terminal(
                gui,
                c::gui_pointer_state_cancelled,
                c::gui_pointer_reason_focus_lost,
                event.tick,
            );
        }
        revision() != before
    }
}

unsafe fn own_window(instance: *mut ProgramInstance, owner: Handle) -> c::GuiPointerWindowIdentity {
    let gui = unsafe { gui(instance) };
    if gui.is_null() {
        return c::GuiPointerWindowIdentity {
            owner,
            window_id: -1,
            ..Default::default()
        };
    }
    unsafe {
        c::GuiPointerWindowIdentity {
            owner,
            epoch: (*gui).pointer.epoch,
            window_id: AtomicI32::from_ptr(ptr::addr_of_mut!((*gui).window_id))
                .load(Ordering::Acquire),
            reserved: 0,
        }
    }
}
/// Existing program metadata admission is retained by the caller.
pub(super) unsafe fn window_epoch(gui: *const t::Gui) -> u64 {
    unsafe { (*gui).pointer.epoch }
}
fn admission() -> Option<task_context::Admission> {
    if threads::compute_worker() {
        return None;
    }
    task_context::Admission::acquire(true)
}
/// # Safety
/// Optional readable exact token and exclusive complete output remain live
/// under the caller's native admission. No snapshot retains an owner pointer.
pub(crate) unsafe extern "C" fn info(token: *const Token, out: *mut Info) -> i32 {
    if out.is_null() {
        return c::gui_pointer_error_invalid;
    }
    let Some(_caller) = admission() else {
        return c::gui_pointer_error_context;
    };
    let Some(owner) = (unsafe { context::handle() }) else {
        return c::gui_pointer_error_context;
    };
    let instance = unsafe { context::current() };
    if instance.is_null() {
        return c::gui_pointer_error_context;
    }
    let requested = if token.is_null() {
        None
    } else {
        Some(unsafe { token.read() })
    };
    if requested.is_some_and(|value| !same(value.window.owner, owner)) {
        return c::gui_pointer_error_stale;
    }
    let physical = mouse::capture_snapshot().0.present;
    let guard = Guard::acquire(0);
    let value = unsafe {
        let gui = gui(instance);
        match requested {
            Some(value) if token_same(value, (*root()).value.capture) => (*root()).value,
            Some(value) if !gui.is_null() && token_same(value, (*gui).pointer.terminal.capture) => {
                (*gui).pointer.terminal
            }
            Some(_) => return c::gui_pointer_error_stale,
            None => {
                let window = own_window(instance, owner);
                if window_same(window, (*root()).value.capture.window)
                    && live((*root()).value.state)
                {
                    (*root()).value
                } else {
                    Info {
                        capture: Token { window, id: 0 },
                        desktop: (*root()).host,
                        revision: (*root()).revision,
                        capabilities: capabilities(physical),
                        ..Default::default()
                    }
                }
            }
        }
    };
    drop(guard);
    unsafe {
        out.write(value);
    }
    0
}
/// # Safety
/// Exact immutable window identity and exclusive token output live through
/// admission. Success creates only PENDING metadata, never active policy.
pub(crate) unsafe extern "C" fn acquire(
    window: *const c::GuiPointerWindowIdentity,
    mode: u32,
    out: *mut Token,
) -> i32 {
    if window.is_null()
        || out.is_null()
        || ![c::gui_pointer_mode_drag, c::gui_pointer_mode_relative].contains(&mode)
    {
        return c::gui_pointer_error_invalid;
    }
    let Some(_caller) = admission() else {
        return c::gui_pointer_error_context;
    };
    let Some(owner) = (unsafe { context::handle() }) else {
        return c::gui_pointer_error_context;
    };
    let instance = unsafe { context::current() };
    if instance.is_null() {
        return c::gui_pointer_error_context;
    }
    let expected = unsafe { window.read() };
    if !same(expected.owner, owner) {
        return c::gui_pointer_error_stale;
    }
    let physical = mouse::capture_snapshot().0.present;
    let guard = Guard::acquire(0);
    unsafe {
        let gui = gui(instance);
        if gui.is_null() {
            return c::gui_pointer_error_unavailable;
        }
        if !window_same(expected, own_window(instance, owner)) {
            return c::gui_pointer_error_stale;
        }
        if super::gui_input::blocked(gui) {
            return c::gui_pointer_error_busy;
        }
        if !valid((*root()).host)
            || !same((*gui).pointer.host, (*root()).host)
            || (*gui).pointer.host_epoch != (*root()).host_epoch
        {
            return c::gui_pointer_error_unavailable;
        }
        let flags = (*gui).window_info.flags;
        // Request is only PENDING. The desktop grant validates actual focus;
        // Down may wake its receiver before the corresponding focus notice.
        // Cooperative process-close publishes under this same metadata owner.
        // A cancelled capture must not be reacquired before window teardown.
        if AtomicBool::from_ptr(ptr::addr_of_mut!((*instance).close_requested))
            .load(Ordering::Acquire)
            || flags & c::GuiWindowFlag::visible == 0
            || flags & (c::GuiWindowFlag::minimized | c::GuiWindowFlag::close_requested) != 0
        {
            return c::gui_pointer_error_unavailable;
        }
        let caps = capabilities(physical);
        let needed = if mode == c::gui_pointer_mode_relative {
            c::gui_pointer_capability_relative_physical
        } else {
            c::gui_pointer_capability_drag
        };
        if caps & needed == 0 {
            return c::gui_pointer_error_unsupported;
        }
        if live((*root()).value.state) || (*gui).pointer.notice_count != 0 {
            return c::gui_pointer_error_busy;
        }
        if (*root()).revision == u64::MAX {
            return c::gui_pointer_error_exhausted;
        }
        let Some(id) = serial() else {
            return c::gui_pointer_error_exhausted;
        };
        let capture = Token {
            window: expected,
            id,
        };
        (*root()).value = Info {
            capture,
            desktop: (*root()).host,
            state: c::gui_pointer_state_pending,
            mode,
            capabilities: caps,
            ..Default::default()
        };
        bump();
        CAPTURE_ID.store(owner.instance_id, Ordering::Release);
        drop(guard);
        out.write(capture);
        signal(gui);
    }
    0
}
/// # Safety
/// Immutable exact own token lives through admission. Last-receipt release
/// is idempotent and cannot mutate a later or foreign binding.
pub(crate) unsafe extern "C" fn release(token: *const Token) -> i32 {
    if token.is_null() {
        return c::gui_pointer_error_invalid;
    }
    let Some(_caller) = admission() else {
        return c::gui_pointer_error_context;
    };
    let Some(owner) = (unsafe { context::handle() }) else {
        return c::gui_pointer_error_context;
    };
    let instance = unsafe { context::current() };
    if instance.is_null() {
        return c::gui_pointer_error_context;
    }
    let token = unsafe { token.read() };
    if !same(token.window.owner, owner) {
        return c::gui_pointer_error_stale;
    }
    let guard = Guard::acquire(0);
    unsafe {
        let gui = gui(instance);
        if gui.is_null() {
            return c::gui_pointer_error_stale;
        }
        if token_same(token, (*root()).value.capture) {
            terminal(
                gui,
                c::gui_pointer_state_released,
                c::gui_pointer_reason_explicit,
                0,
            );
        } else if !token_same(token, (*gui).pointer.terminal.capture) {
            return c::gui_pointer_error_stale;
        }
        drop(guard);
        signal(gui);
    }
    0
}

/// Registry is already held before metadata0. No returned pointer escapes
/// those guards; notifications reacquire their own generation-bound lease.
unsafe fn destination(owner: Handle) -> *mut t::Gui {
    unsafe {
        let slot = registry::lookup_handle(owner, true);
        if slot.is_null() || !registry::running((*slot).state) {
            return ptr::null_mut();
        }
        let instance = ptr::addr_of_mut!((*slot).instance);
        if AtomicBool::from_ptr(ptr::addr_of_mut!((*instance).done)).load(Ordering::Acquire) {
            return ptr::null_mut();
        }
        gui(instance)
    }
}
unsafe fn notify_owner(owner: Handle) {
    super::gui_activity::signal_handle(owner);
}
unsafe fn signal_owner(owner: Handle) {
    unsafe {
        notify_owner(owner);
    }
    desktop_events::r4os_rust_desktop_signal();
}
unsafe fn window_current(gui: *mut t::Gui, token: Token) -> bool {
    unsafe {
        !gui.is_null()
            && (*gui).pointer.epoch == token.window.epoch
            && (*gui).pointer.host_epoch == (*root()).host_epoch
            && same((*gui).pointer.host, (*root()).host)
            && AtomicI32::from_ptr(ptr::addr_of_mut!((*gui).window_id)).load(Ordering::Acquire)
                == token.window.window_id
    }
}
unsafe fn host_locked(
    source: Handle,
    request: c::GuiPointerHostRequest,
    physical: mouse::State,
    totals: u64,
) -> Result<(Info, Handle, bool), i32> {
    unsafe {
        let root = root();
        let owner = (*root).value.capture.window.owner;
        let gui = destination(owner);
        let mut changed = false;
        if request.operation == c::gui_pointer_host_bind {
            let all = c::gui_pointer_capability_drag
                | c::gui_pointer_capability_relative_physical
                | c::gui_pointer_capability_absolute_remote_drag
                | c::gui_pointer_capability_wheel_events;
            if request.flags & !all != 0
                || request.flags & c::gui_pointer_capability_drag == 0
                || ![
                    c::gui_pointer_reason_none,
                    c::gui_pointer_reason_desktop_changed,
                ]
                .contains(&request.reason)
            {
                return Err(c::gui_pointer_error_invalid);
            }
            if valid((*root).host) && !same((*root).host, source) {
                let slot = registry::lookup_handle((*root).host, true);
                if !slot.is_null()
                    && registry::running((*slot).state)
                    && !AtomicBool::from_ptr(ptr::addr_of_mut!((*slot).instance.done))
                        .load(Ordering::Acquire)
                {
                    return Err(c::gui_pointer_error_busy);
                }
            }
            if !same((*root).host, source)
                || request.reason == c::gui_pointer_reason_desktop_changed
            {
                if (*root).revision == u64::MAX {
                    return Err(c::gui_pointer_error_exhausted);
                }
                let epoch = serial().ok_or(c::gui_pointer_error_exhausted)?;
                terminal(
                    gui,
                    c::gui_pointer_state_cancelled,
                    c::gui_pointer_reason_desktop_changed,
                    request.event.tick,
                );
                (*root).host = source;
                (*root).host_epoch = epoch;
                HOST_ID.store(source.instance_id, Ordering::Release);
                (*root).host_capabilities = request.flags;
                bump();
                changed = true;
            } else if (*root).host_capabilities != request.flags {
                (*root).host_capabilities = request.flags;
                bump();
                changed = true;
            }
        } else {
            if !same((*root).host, source) {
                return Err(c::gui_pointer_error_stale);
            }
            if request.operation == c::gui_pointer_host_unbind {
                terminal(
                    gui,
                    c::gui_pointer_state_cancelled,
                    c::gui_pointer_reason_desktop_changed,
                    request.event.tick,
                );
                (*root).host = Handle::default();
                (*root).host_epoch = 0;
                (*root).host_capabilities = 0;
                bump();
                changed = true;
                HOST_ID.store(0, Ordering::Release);
            } else if request.operation == c::gui_pointer_host_snapshot {
                if live((*root).value.state) {
                    if gui.is_null() {
                        terminal(
                            gui,
                            c::gui_pointer_state_cancelled,
                            c::gui_pointer_reason_process_ended,
                            request.event.tick,
                        );
                        changed = true;
                    } else if !window_current(gui, (*root).value.capture) {
                        terminal(
                            gui,
                            c::gui_pointer_state_cancelled,
                            c::gui_pointer_reason_window_closed,
                            request.event.tick,
                        );
                        changed = true;
                    }
                }
            } else {
                if !token_same(request.capture, (*root).value.capture)
                    || !same(source, (*root).value.desktop)
                {
                    return Err(c::gui_pointer_error_stale);
                }
                match request.operation {
                    c::gui_pointer_host_grant => {
                        if (*root).value.state == c::gui_pointer_state_pending {
                            // Focus policy belongs to the exact desktop host. A native
                            // focus-loss notice independently terminates a live token.
                            if !window_current(gui, request.capture)
                                || (*gui).window_info.flags & c::GuiWindowFlag::visible == 0
                                || (*gui).window_info.flags
                                    & (c::GuiWindowFlag::minimized
                                        | c::GuiWindowFlag::close_requested)
                                    != 0
                            {
                                terminal(
                                    gui,
                                    c::gui_pointer_state_cancelled,
                                    c::gui_pointer_reason_window_closed,
                                    request.event.tick,
                                );
                            } else if (*root).value.mode == c::gui_pointer_mode_relative
                                && capabilities(physical.present)
                                    & c::gui_pointer_capability_relative_physical
                                    == 0
                            {
                                terminal(
                                    gui,
                                    c::gui_pointer_state_cancelled,
                                    c::gui_pointer_reason_source_unsupported,
                                    request.event.tick,
                                );
                            } else {
                                if (*root).revision == u64::MAX {
                                    return Err(c::gui_pointer_error_exhausted);
                                }
                                (*root).value.state = c::gui_pointer_state_active;
                                (*root).value.buttons = u32::from(physical.buttons);
                                (*root).value.motion_x = totals as u32;
                                (*root).value.motion_y = (totals >> 32) as u32;
                                (*root).value.cursor_x = request.event.x;
                                (*root).value.cursor_y = request.event.y;
                                bump();
                                notice(gui, (*root).value, request.event.tick);
                            }
                            changed = true;
                        } else if (*root).value.state != c::gui_pointer_state_active {
                            return Err(c::gui_pointer_error_stale);
                        }
                    }
                    c::gui_pointer_host_cancel => {
                        if request.reason == 0
                            || request.reason > c::gui_pointer_reason_input_overflow
                        {
                            return Err(c::gui_pointer_error_invalid);
                        }
                        let was = live((*root).value.state);
                        terminal(
                            gui,
                            c::gui_pointer_state_cancelled,
                            request.reason,
                            request.event.tick,
                        );
                        changed = was;
                    }
                    c::gui_pointer_host_deliver => {
                        if (*root).value.state != c::gui_pointer_state_active
                            || !window_current(gui, request.capture)
                        {
                            return Err(c::gui_pointer_error_stale);
                        }
                        let event = request.event;
                        if event.window_id != request.capture.window.window_id
                            || ![
                                c::gui_pointer_source_physical_relative,
                                c::gui_pointer_source_remote_absolute,
                            ]
                            .contains(&request.flags)
                        {
                            return Err(c::gui_pointer_error_invalid);
                        }
                        let relative = event.kind == c::GuiEventKind::mouse_relative.0;
                        let motion = event.kind == c::GuiEventKind::mouse_move.0;
                        let ordinary = [
                            c::GuiEventKind::mouse_down.0,
                            c::GuiEventKind::mouse_up.0,
                            c::GuiEventKind::mouse_wheel.0,
                        ]
                        .contains(&event.kind);
                        if relative
                            && ((*root).value.mode != c::gui_pointer_mode_relative
                                || request.flags != c::gui_pointer_source_physical_relative)
                            || motion && (*root).value.mode != c::gui_pointer_mode_drag
                            || !relative && !motion && !ordinary
                        {
                            return Err(c::gui_pointer_error_unsupported);
                        }
                        if gui_events::enqueue_locked(gui, event) != 0 {
                            return Err(c::gui_pointer_error_queue_full);
                        }
                        (*root).value.buttons = event.buttons;
                        changed = true;
                    }
                    _ => return Err(c::gui_pointer_error_invalid),
                }
            }
        }
        let mut value = (*root).value;
        if (request.operation == c::gui_pointer_host_snapshot
            && request.flags & c::gui_pointer_host_flag_motion_snapshot != 0)
            || [
                c::gui_pointer_host_bind,
                c::gui_pointer_host_cancel,
                c::gui_pointer_host_unbind,
            ]
            .contains(&request.operation)
        {
            value.motion_x = totals as u32;
            value.motion_y = (totals >> 32) as u32;
        }
        value.capabilities = capabilities(physical.present);
        if value.capture.id == 0 {
            value.desktop = (*root).host;
            value.revision = (*root).revision;
        }
        Ok((value, owner, changed))
    }
}
/// # Safety
/// Readable complete request/exclusive output. Registry lookup/retirement is
/// pinned before metadata0; neither guard spans notification or caller output.
/// BIND establishes a functional host generation, not an OS permission model.
pub(crate) unsafe extern "C" fn host(
    request: *const c::GuiPointerHostRequest,
    out: *mut Info,
) -> i32 {
    if request.is_null() || out.is_null() {
        return c::gui_pointer_error_invalid;
    }
    let request = unsafe { request.read() };
    if request.reserved != 0 || request.operation > c::gui_pointer_host_unbind {
        return c::gui_pointer_error_invalid;
    }
    let allowed_flags = match request.operation {
        c::gui_pointer_host_bind => u32::MAX,
        c::gui_pointer_host_deliver => u32::MAX,
        c::gui_pointer_host_snapshot => c::gui_pointer_host_flag_motion_snapshot,
        _ => 0,
    };
    if request.flags & !allowed_flags != 0 {
        return c::gui_pointer_error_invalid;
    }
    let Some(_caller) = admission() else {
        return c::gui_pointer_error_context;
    };
    let Some(source) = (unsafe { context::handle() }) else {
        return c::gui_pointer_error_context;
    };
    // Idle Desktop polling needs no registry traversal or input consumption.
    if request.operation == c::gui_pointer_host_snapshot {
        let guard = Guard::acquire(0);
        unsafe {
            if same((*root()).host, source) && !live((*root()).value.state) {
                let mut value = (*root()).value;
                drop(guard);
                if request.flags & c::gui_pointer_host_flag_motion_snapshot != 0 {
                    let (_, totals) = mouse::capture_snapshot();
                    value.motion_x = totals as u32;
                    value.motion_y = (totals >> 32) as u32;
                }
                out.write(value);
                return 0;
            }
        }
    }
    let (physical, totals) = mouse::capture_snapshot();
    let Some(registry) = super::registry::Guard::acquire() else {
        return c::gui_pointer_error_context;
    };
    let guard = Guard::acquire(0);
    let result = unsafe { host_locked(source, request, physical, totals) };
    drop(guard);
    drop(registry);
    match result {
        Ok((value, owner, changed)) => {
            unsafe {
                out.write(value);
            }
            if changed {
                unsafe {
                    if request.operation == c::gui_pointer_host_deliver {
                        notify_owner(owner);
                    } else {
                        signal_owner(owner);
                    }
                }
            }
            0
        }
        Err(error) => error,
    }
}
/// Nonblocking ordinary host input. Registry precedes metadata exactly as in
/// native capture; neither owner spans notification or caller output.
pub(crate) unsafe extern "C" fn host_input(
    request: *const c::GuiHostInputRequest,
    out: *mut c::GuiPointerWindowIdentity,
) -> i32 {
    if request.is_null() || out.is_null() {
        return c::gui_host_input_error_invalid;
    }
    let request = unsafe { request.read() };
    if request.operation > c::gui_host_input_operation_reset
        || request.window.reserved != 0
        || !valid(request.window.owner)
        || request.window.window_id < 0
        || (request.operation != c::gui_host_input_operation_reset && request.reason != 0)
    {
        return c::gui_host_input_error_invalid;
    }
    let zero_event = request.event.kind == 0
        && matches!(request.event.window_id, -1 | 0)
        && request.event.x == 0
        && request.event.y == 0
        && request.event.key == 0
        && request.event.buttons == 0
        && request.event.modifiers == 0;
    if request.operation == c::gui_host_input_operation_query
        && (request.window.epoch != 0 || !zero_event || request.event.tick != 0)
    {
        return c::gui_host_input_error_invalid;
    }
    if request.operation == c::gui_host_input_operation_reset
        && (!zero_event
            || ![
                c::gui_input_reset_reason_overflow,
                c::gui_input_reset_reason_focus_lost,
                c::gui_input_reset_reason_window_closed,
                c::gui_input_reset_reason_desktop_changed,
                c::gui_input_reset_reason_process_ended,
            ]
            .contains(&request.reason))
    {
        return c::gui_host_input_error_invalid;
    }
    if request.operation == c::gui_host_input_operation_post
        && (request.event.kind == 0
            || request.event.kind > c::GuiEventKind::mouse_wheel.0
            || request.event.window_id != request.window.window_id)
    {
        return c::gui_host_input_error_invalid;
    }
    let Some(_caller) = admission() else {
        return c::gui_host_input_error_context;
    };
    let Some(source) = (unsafe { context::handle() }) else {
        return c::gui_host_input_error_context;
    };
    let Some(registry) = registry::Guard::acquire() else {
        return c::gui_host_input_error_context;
    };
    let guard = Guard::acquire(0);
    let mut capture_changed = false;
    let result = unsafe {
        (|| -> Result<(c::GuiPointerWindowIdentity, bool), i32> {
            if !same((*root()).host, source) {
                return Err(c::gui_host_input_error_context);
            }
            let gui = destination(request.window.owner);
            if gui.is_null() {
                return Err(c::gui_host_input_error_stale);
            }
            let slot = registry::lookup_handle(request.window.owner, true);
            let identity = own_window(ptr::addr_of_mut!((*slot).instance), request.window.owner);
            if identity.epoch == 0
                || identity.window_id != request.window.window_id
                || !same((*gui).pointer.host, source)
                || (*gui).pointer.host_epoch != (*root()).host_epoch
                || (request.operation != c::gui_host_input_operation_query
                    && !window_same(identity, request.window))
            {
                return Err(c::gui_host_input_error_stale);
            }
            match request.operation {
                c::gui_host_input_operation_query => Ok((identity, false)),
                c::gui_host_input_operation_post => {
                    if request.event.kind == c::GuiEventKind::focus_gained.0
                        || request.event.kind == c::GuiEventKind::focus_lost.0
                    {
                        capture_changed |= focus_locked(gui, identity.owner, request.event, source);
                    }
                    if super::gui_input::ordinary(request.event.kind)
                        && ((*gui).window_info.flags & c::GuiWindowFlag::visible == 0
                            || (*gui).window_info.flags
                                & (c::GuiWindowFlag::minimized | c::GuiWindowFlag::close_requested)
                                != 0)
                    {
                        return Err(c::gui_host_input_error_stale);
                    }
                    if super::gui_input::blocked(gui) {
                        super::gui_input::arm(gui);
                        return Err(c::gui_host_input_error_busy);
                    }
                    let code = gui_events::enqueue_locked(gui, request.event);
                    if code == -3 {
                        super::gui_input::arm(gui);
                    }
                    if code != 0 {
                        return Err(code);
                    }
                    Ok((identity, true))
                }
                c::gui_host_input_operation_reset => {
                    let before = revision();
                    if same((*root()).value.capture.window.owner, identity.owner)
                        && live((*root()).value.state)
                    {
                        let reason = match request.reason {
                            c::gui_input_reset_reason_overflow => {
                                c::gui_pointer_reason_input_overflow
                            }
                            c::gui_input_reset_reason_focus_lost => {
                                c::gui_pointer_reason_focus_lost
                            }
                            c::gui_input_reset_reason_window_closed => {
                                c::gui_pointer_reason_window_closed
                            }
                            c::gui_input_reset_reason_desktop_changed => {
                                c::gui_pointer_reason_desktop_changed
                            }
                            _ => c::gui_pointer_reason_process_ended,
                        };
                        terminal(
                            gui,
                            c::gui_pointer_state_cancelled,
                            reason,
                            request.event.tick,
                        );
                    }
                    if request.reason != c::gui_input_reset_reason_overflow {
                        (*gui).pointer.focused = false;
                    }
                    super::gui_input::reset(gui, identity, request.reason, request.event.tick);
                    capture_changed |= revision() != before;
                    Ok((identity, true))
                }
                _ => unreachable!(),
            }
        })()
    };
    drop(guard);
    drop(registry);
    if capture_changed {
        unsafe { signal_owner(request.window.owner) };
    }
    match result {
        Ok((identity, changed)) => {
            unsafe { out.write(identity) };
            if changed && !capture_changed {
                unsafe { notify_owner(identity.owner) };
            }
            0
        }
        Err(code) => code,
    }
}

/// # Safety
/// Exact ending owner; caller holds no registry mutex. Live recipients are
/// found while Registry is held, before metadata0, then signalled by lease.
pub(super) unsafe fn orphan(owner: Handle) {
    if HOST_ID.load(Ordering::Acquire) != owner.instance_id
        && CAPTURE_ID.load(Ordering::Acquire) != owner.instance_id
    {
        return;
    }
    let Some(registry) = super::registry::Guard::acquire() else {
        return;
    };
    let guard = Guard::acquire(0);
    let mut notify = Handle::default();
    let mut host_ended = false;
    unsafe {
        if live((*root()).value.state)
            && (same((*root()).value.capture.window.owner, owner)
                || same((*root()).value.desktop, owner))
        {
            notify = (*root()).value.capture.window.owner;
            let gui = if same(notify, owner) {
                ptr::null_mut()
            } else {
                destination(notify)
            };
            terminal(
                gui,
                c::gui_pointer_state_cancelled,
                if same(notify, owner) {
                    c::gui_pointer_reason_process_ended
                } else {
                    c::gui_pointer_reason_desktop_changed
                },
                0,
            );
        }
        if same((*root()).host, owner) {
            host_ended = true;
            (*root()).host = Handle::default();
            (*root()).host_epoch = 0;
            (*root()).host_capabilities = 0;
            bump();
            HOST_ID.store(0, Ordering::Release);
        }
    }
    drop(guard);
    drop(registry);
    unsafe {
        signal_owner(notify);
        if host_ended {
            orphan_host_inputs(owner);
        }
    }
}
/// Stream exact handles by boot-unique generation. No registry chunk pointer
/// crosses unlock/notification: empty chunks can actually be reclaimed. No
/// fixed recipient snapshot truncates a formerly hosted GUI's cancellation.
unsafe fn orphan_host_inputs(host: Handle) {
    let mut after = 0u64;
    loop {
        let Some(registry) = registry::Guard::acquire() else {
            return;
        };
        let guard = Guard::acquire(0);
        let mut next = Handle::default();
        unsafe {
            let mut chunk = (*super::registry_state::state()).head;
            while !chunk.is_null() {
                for slot in &(*chunk).slots {
                    if !registry::running(slot.state)
                        || slot.generation <= after
                        || (next.generation != 0 && slot.generation >= next.generation)
                    {
                        continue;
                    }
                    let gui = slot.instance.gui_payload.cast::<t::Gui>();
                    if !gui.is_null()
                        && same((*gui).pointer.host, host)
                        && (*gui).pointer.epoch != 0
                    {
                        next = registry::handle_for_slot(slot);
                    }
                }
                chunk = (*chunk).next;
            }
        }
        drop(guard);
        drop(registry);
        if !valid(next) {
            return;
        }
        after = next.generation;
        let Some(lease) = (unsafe { super::console::Lease::handle(next) }) else {
            continue;
        };
        let gui = unsafe { (*lease.0.instance).gui_payload.cast::<t::Gui>() };
        if gui.is_null() {
            continue;
        }
        let guard = Guard::acquire(0);
        let changed = unsafe {
            if same((*gui).pointer.host, host)
                && (*gui).pointer.epoch != 0
                && !AtomicBool::from_ptr(ptr::addr_of_mut!((*lease.0.instance).done))
                    .load(Ordering::Acquire)
            {
                let window = own_window(lease.0.instance, next);
                (*gui).pointer.focused = false;
                super::gui_input::reset(gui, window, c::gui_input_reset_reason_desktop_changed, 0);
                true
            } else {
                false
            }
        };
        drop(guard);
        if changed {
            unsafe { super::gui_activity::signal(gui) };
        }
        // Lease owns the exact payload until after this one notification.
    }
}
pub(super) unsafe fn revision() -> u64 {
    unsafe { (*root()).revision }
}
