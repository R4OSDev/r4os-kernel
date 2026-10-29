//! Reliable ordinary input cancellation and demand-driven host backpressure.
//! The existing program metadata owner guards every field. Policy and pending
//! event storage stay in Desktop; this state retains only a native barrier.
use super::{gui_events, payload_types as t};
use crate::contract as c;
use core::mem;

#[repr(C)]
pub struct Control {
    event: c::GuiEvent,
    epoch: u64,
    pending: bool,
    waiting: bool,
    had_window: bool,
}
impl Control {
    // Integer/bool-only record. Zero owns no event, window or wait obligation.
    pub const EMPTY: Self = unsafe { mem::zeroed() };
}
pub(super) fn ordinary(kind: u32) -> bool {
    [
        c::GuiEventKind::key_down.0,
        c::GuiEventKind::mouse_down.0,
        c::GuiEventKind::mouse_up.0,
        c::GuiEventKind::mouse_move.0,
        c::GuiEventKind::physical_key_down.0,
        c::GuiEventKind::physical_key_up.0,
        c::GuiEventKind::physical_key_reset.0,
        c::GuiEventKind::mouse_wheel.0,
        c::GuiEventKind::mouse_relative.0,
    ]
    .contains(&kind)
}
/// A new native window incarnation cannot inherit the former window's unread
/// records or barrier. Capture receipts remain in their separate exact-token
/// channel; original removal hooks make any required old prefix empty.
pub(super) unsafe fn rebind(gui: *mut t::Gui, window: c::GuiPointerWindowIdentity, reason: u32) {
    unsafe {
        let had_window = (*gui).input.had_window;
        let unread = ((*gui).event_tail + 64 - (*gui).event_head) % 64;
        for _ in 0..unread {
            gui_events::remove(gui, (*gui).event_head);
        }
        // This is a fresh barrier for the new incarnation, not replay of an
        // old event. An already running app must cancel former held state.
        (*gui).input = Control {
            event: c::GuiEvent {
                kind: c::GuiEventKind::input_reset.0,
                window_id: window.window_id,
                key: reason,
                ..Default::default()
            },
            epoch: window.epoch,
            pending: had_window && window.epoch != 0,
            had_window: had_window || window.epoch != 0,
            ..Control::EMPTY
        };
    }
}
/// Caller retains metadata0. A reset is not a replay of an accepted prefix.
/// Remove unread ordinary input through the original removal hook so native
/// capture notices keep their exact place relative to retained metadata.
pub(super) unsafe fn reset(
    gui: *mut t::Gui,
    window: c::GuiPointerWindowIdentity,
    reason: u32,
    tick: u64,
) {
    unsafe {
        let mut index = (*gui).event_head;
        while index != (*gui).event_tail {
            if ordinary((*gui).events[index].kind) {
                gui_events::remove(gui, index);
            } else {
                index = (index + 1) % 64;
            }
        }
        let control = &mut (*gui).input;
        control.event = c::GuiEvent {
            kind: c::GuiEventKind::input_reset.0,
            window_id: window.window_id,
            key: reason,
            tick,
            ..Default::default()
        };
        control.epoch = window.epoch;
        control.pending = true;
    }
}
/// No pointer survives the enclosing existing program metadata admission.
pub(super) unsafe fn blocked(gui: *const t::Gui) -> bool {
    unsafe { (*gui).input.pending }
}
pub(super) unsafe fn arm(gui: *mut t::Gui) {
    unsafe { (*gui).input.waiting = true };
}
/// After metadata is released, the caller signals Desktop only for an actual
/// rejected host post. Ordinary successful polls add no wakeup or statistic.
pub(super) unsafe fn space(gui: *mut t::Gui) -> bool {
    unsafe { mem::replace(&mut (*gui).input.waiting, false) }
}
pub(super) unsafe fn pending(gui: *const t::Gui, epoch: u64) -> u32 {
    unsafe { u32::from((*gui).input.pending && (*gui).input.epoch == epoch) }
}
/// Native capture controls are polled first. This barrier follows them and
/// precedes all remaining metadata and any later ordinary event admission.
pub(super) unsafe fn poll(gui: *mut t::Gui, epoch: u64) -> Option<c::GuiEvent> {
    unsafe {
        let control = &mut (*gui).input;
        if !control.pending {
            return None;
        }
        control.pending = false;
        (control.epoch == epoch).then_some(control.event)
    }
}
const _: () = assert!(mem::size_of::<Control>() == 56);
