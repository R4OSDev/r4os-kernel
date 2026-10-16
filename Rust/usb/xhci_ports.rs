//! Original port debounce/power/reset and hotplug lifecycle. Mutable port
//! snapshots are caller-local copies, never Rust borrows into live status over
//! a wait. Detached transactions retain DMA until their exact terminal exit.
use super::{
    event_router::{PortAction, decide_port_action},
    timing,
    wait_types::{Deadline, Wait},
    xhci_constants::*,
    xhci_io::*,
    xhci_runtime::*,
    xhci_state::{current, state},
    xhci_types::PortStatus,
};
use crate::sched::{scheduler, sync_types::Text};
use core::ptr;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PreparePhase {
    Startup,
    Probe,
}
#[derive(Default)]
pub(super) struct Outcome {
    pub catalog_changed: bool,
    pub retry: bool,
}
pub(super) fn port_link_state(portsc: u32) -> u32 {
    (portsc & PORTSC_PLS_MASK) >> 5
}

fn phase_reason(phase: PreparePhase, startup: &'static [u8], probe: &'static [u8]) -> Text {
    Text::literal(match phase {
        PreparePhase::Startup => startup,
        PreparePhase::Probe => probe,
    })
}
pub(super) unsafe fn refresh_port_status(p: &mut PortStatus, index: usize) {
    unsafe {
        let base = OP_PORT_BASE + index as u64 * OP_PORT_STRIDE;
        p.portsc = read_op32(base);
        p.portpmsc = read_op32(base + 4);
        p.portli = read_op32(base + 8);
        p.porthlpmc = read_op32(base + 12);
        p.change_bits = p.portsc & PORTSC_CHANGE_MASK;
        p.link_state = port_link_state(p.portsc) as u8;
        p.connected = p.portsc & PORTSC_CCS != 0;
        p.enabled = p.portsc & PORTSC_PED != 0;
        p.powered = p.portsc & PORTSC_PP != 0;
    }
}
pub(super) unsafe fn clear_port_changes(index: usize) -> u32 {
    unsafe {
        let base = OP_PORT_BASE + index as u64 * OP_PORT_STRIDE;
        let portsc = read_op32(base);
        let changes = portsc & PORTSC_CHANGE_MASK;
        if changes != 0 {
            write_op32(base, (portsc & PORTSC_WRITE_PRESERVE_MASK) | changes);
            (*current()).port_change_clears += 1;
        }
        changes
    }
}
pub(super) unsafe fn store_port_snapshot(index: usize, snapshot: PortStatus) {
    unsafe {
        if index >= usize::from((*current()).port_count_seen) || index >= MAX_PORTS {
            return;
        }
        let previous = ptr::addr_of!((*current()).first_ports[index]).read();
        if previous.connected != snapshot.connected {
            if snapshot.connected {
                (*current()).connected_ports = (*current()).connected_ports.saturating_add(1);
            } else if (*current()).connected_ports > 0 {
                (*current()).connected_ports -= 1;
            }
        }
        if previous.enabled != snapshot.enabled {
            if snapshot.enabled {
                (*current()).enabled_ports = (*current()).enabled_ports.saturating_add(1);
            } else if (*current()).enabled_ports > 0 {
                (*current()).enabled_ports -= 1;
            }
        }
        ptr::addr_of_mut!((*current()).first_ports[index]).write(snapshot);
    }
}
pub(super) unsafe fn refresh_port_snapshot_by_number(port: u8) {
    unsafe {
        if port == 0 || port > (*current()).port_count_seen {
            return;
        }
        let index = usize::from(port - 1);
        let mut snapshot = ptr::addr_of!((*current()).first_ports[index]).read();
        refresh_port_status(&mut snapshot, index);
        store_port_snapshot(index, snapshot);
    }
}
pub(super) unsafe fn acknowledge_port_changes(index: usize) {
    unsafe {
        clear_port_changes(index);
        let mut p = ptr::addr_of!((*current()).first_ports[index]).read();
        refresh_port_status(&mut p, index);
        store_port_snapshot(index, p);
    }
}
pub(super) unsafe fn port_is_connected(port: u8) -> bool {
    unsafe {
        for index in 0..usize::from((*current()).port_count_seen) {
            let p = ptr::addr_of!((*current()).first_ports[index]).read();
            if p.index == port {
                return p.connected;
            }
        }
        false
    }
}

pub(super) unsafe fn debounce_connected_port(index: usize, p: &mut PortStatus) -> bool {
    unsafe {
        let base = OP_PORT_BASE + index as u64 * OP_PORT_STRIDE;
        let observed = PORTSC_CCS | PORTSC_PED | PORTSC_PLS_MASK | PORTSC_PR | PORTSC_WPR;
        let mut last = read_op32(base);
        let mut wait = Wait::begin(
            timing::XHCI_PORT_DEBOUNCE_TIMEOUT_MS,
            Text::literal(b"xhci-port-debounce"),
            0,
        );
        let mut stable = Deadline::begin(timing::XHCI_PORT_DEBOUNCE_STABLE_MS);
        while !wait.expired() {
            let now = read_op32(base);
            if now & PORTSC_CCS == 0 {
                stable.finish();
                wait.finish(false);
                refresh_port_status(p, index);
                return false;
            }
            if now & observed != last & observed {
                stable.finish();
                stable = Deadline::begin(timing::XHCI_PORT_DEBOUNCE_STABLE_MS);
                last = now;
            } else if stable.expired_any() {
                stable.finish();
                wait.finish(true);
                refresh_port_status(p, index);
                p.debounce_ok = true;
                return true;
            }
            if super::xhci::irq_registered() && !scheduler::current().is_null() {
                if super::xhci::wait_event_irq(EVENT_IRQ_POLL_TICKS) {
                    super::xhci::record_irq_wakeup();
                } else {
                    super::xhci::record_poll_fallback();
                }
            } else {
                super::xhci::record_poll_fallback();
                wait.idle();
            }
        }
        stable.finish();
        wait.finish(false);
        refresh_port_status(p, index);
        p.debounce_ok = false;
        false
    }
}
pub(super) unsafe fn ensure_port_power(index: usize, p: &mut PortStatus) -> bool {
    unsafe {
        if p.powered {
            return true;
        }
        let base = OP_PORT_BASE + index as u64 * OP_PORT_STRIDE;
        (*current()).port_power_requests += 1;
        let before = read_op32(base);
        write_op32(
            base,
            (before & PORTSC_WRITE_PRESERVE_MASK) | PORTSC_PP | (before & PORTSC_CHANGE_MASK),
        );
        let mut wait = Wait::begin(
            timing::XHCI_PORT_POWER_TIMEOUT_MS,
            Text::literal(b"xhci-port-power"),
            0,
        );
        while !wait.expired() {
            refresh_port_status(p, index);
            if !p.connected {
                wait.finish(false);
                return false;
            }
            if p.powered {
                wait.finish(true);
                return true;
            }
            wait.idle();
        }
        wait.finish(false);
        (*current()).timeouts += 1;
        false
    }
}
pub(super) unsafe fn reset_port(index: u8) -> bool {
    unsafe {
        if let Some(runtime) = runtime_index_for_port(index.saturating_add(1))
            && (*state()).runtimes[runtime].transaction_active
        {
            return false;
        }
        let base = OP_PORT_BASE + u64::from(index) * OP_PORT_STRIDE;
        let before = read_op32(base);
        clear_port_changes(usize::from(index));
        write_op32(base, (before & PORTSC_WRITE_PRESERVE_MASK) | PORTSC_PR);
        let mut wait = Wait::begin(
            timing::XHCI_PORT_RESET_TIMEOUT_MS,
            Text::literal(b"xhci-port-reset"),
            0,
        );
        while !wait.expired() {
            let portsc = read_op32(base);
            if portsc & PORTSC_PR == 0 && (portsc & PORTSC_PED != 0 || portsc & PORTSC_CCS == 0) {
                clear_port_changes(usize::from(index));
                wait.finish(true);
                return true;
            }
            wait.idle();
        }
        wait.finish(false);
        (*current()).timeouts += 1;
        false
    }
}
pub(super) unsafe fn warm_reset_port(index: u8) -> bool {
    unsafe {
        if let Some(runtime) = runtime_index_for_port(index.saturating_add(1))
            && (*state()).runtimes[runtime].transaction_active
        {
            return false;
        }
        let base = OP_PORT_BASE + u64::from(index) * OP_PORT_STRIDE;
        let before = read_op32(base);
        clear_port_changes(usize::from(index));
        write_op32(base, (before & PORTSC_WRITE_PRESERVE_MASK) | PORTSC_WPR);
        let mut wait = Wait::begin(
            timing::XHCI_PORT_WARM_RESET_TIMEOUT_MS,
            Text::literal(b"xhci-port-warm-reset"),
            0,
        );
        while !wait.expired() {
            let portsc = read_op32(base);
            if portsc & PORTSC_WPR == 0 {
                clear_port_changes(usize::from(index));
                let ready = portsc & PORTSC_CCS == 0
                    || (portsc & PORTSC_PED != 0 && port_link_state(portsc) == PORTSC_PLS_U0);
                wait.finish(ready);
                return ready;
            }
            wait.idle();
        }
        wait.finish(false);
        (*current()).timeouts += 1;
        false
    }
}
pub(super) unsafe fn wait_port_u0(index: u8, p: &mut PortStatus) -> bool {
    unsafe {
        (*current()).port_u0_waits += 1;
        let mut wait = Wait::begin(
            timing::XHCI_PORT_U0_TIMEOUT_MS,
            Text::literal(b"xhci-port-u0"),
            0,
        );
        while !wait.expired() {
            refresh_port_status(p, usize::from(index));
            if p.portsc & PORTSC_CCS == 0 {
                wait.finish(false);
                return false;
            }
            if p.portsc & PORTSC_PED != 0 && port_link_state(p.portsc) == PORTSC_PLS_U0 {
                wait.finish(true);
                return true;
            }
            wait.idle();
        }
        wait.finish(false);
        false
    }
}
pub(super) unsafe fn prepare_port_state(
    index: usize,
    p: &mut PortStatus,
    phase: PreparePhase,
) -> bool {
    unsafe {
        refresh_port_status(p, index);
        let debounced = p.debounce_ok;
        let changes = clear_port_changes(index);
        if changes != 0 {
            p.change_bits |= changes;
            refresh_port_status(p, index);
        }
        if !p.connected {
            p.debounce_ok = false;
            p.reset_reason = Text::literal(b"no-device");
            return false;
        }
        p.debounce_ok = debounced && changes & PORTSC_CSC == 0;
        if !p.debounce_ok && !debounce_connected_port(index, p) {
            p.reset_reason = phase_reason(phase, b"debounce-timeout", b"probe-debounce-timeout");
            (*current()).port_debounce_failures += 1;
            return false;
        }
        if !ensure_port_power(index, p) {
            p.reset_reason = phase_reason(phase, b"power-timeout", b"probe-power-timeout");
            return false;
        }
        let force = phase == PreparePhase::Startup && !p.reset_attempted;
        if !p.enabled || force {
            p.reset_attempted = true;
            p.reset_ok = reset_port(u8::try_from(index).expect("USB port index"));
            p.reset_reason = phase_reason(
                phase,
                if p.reset_ok {
                    b"reset-enabled"
                } else {
                    b"reset-timeout"
                },
                if p.reset_ok {
                    b"probe-reset-enabled"
                } else {
                    b"probe-reset-timeout"
                },
            );
            if p.reset_ok {
                (*current()).reset_ports += 1;
            }
            refresh_port_status(p, index);
            if p.reset_ok && p.connected && p.enabled {
                super::wait::milliseconds(
                    timing::PORT_RESET_RECOVERY_MS,
                    Text::literal(b"xhci-port-reset-recovery"),
                    0,
                );
                refresh_port_status(p, index);
            }
        }
        if p.enabled && port_link_state(p.portsc) != PORTSC_PLS_U0 {
            if wait_port_u0(u8::try_from(index).expect("USB port index"), p) {
                p.reset_reason = phase_reason(phase, b"waited-u0", b"probe-waited-u0");
            } else {
                p.reset_attempted = true;
                p.reset_ok = warm_reset_port(u8::try_from(index).expect("USB port index"));
                (*current()).port_warm_resets += 1;
                p.reset_reason = phase_reason(
                    phase,
                    if p.reset_ok {
                        b"warm-reset-u0"
                    } else {
                        b"warm-reset-timeout"
                    },
                    if p.reset_ok {
                        b"probe-warm-reset-u0"
                    } else {
                        b"probe-warm-reset-timeout"
                    },
                );
                if p.reset_ok {
                    (*current()).reset_ports += 1;
                }
                refresh_port_status(p, index);
                if p.connected
                    && p.enabled
                    && port_link_state(p.portsc) != PORTSC_PLS_U0
                    && wait_port_u0(u8::try_from(index).expect("USB port index"), p)
                {
                    p.reset_reason =
                        phase_reason(phase, b"warm-reset-waited-u0", b"probe-warm-waited-u0");
                }
            }
        } else if p.connected && p.enabled && (p.reset_reason.len == 0 || !p.reset_attempted) {
            p.reset_reason = phase_reason(phase, b"already-enabled", b"probe-already-enabled");
        }
        clear_port_changes(index);
        refresh_port_status(p, index);
        p.connected && p.enabled && port_link_state(p.portsc) == PORTSC_PLS_U0
    }
}
pub(super) unsafe fn prepare_port_for_probe(index: usize) -> bool {
    unsafe {
        if index >= usize::from((*current()).port_count_seen) {
            return false;
        }
        let mut p = ptr::addr_of!((*current()).first_ports[index]).read();
        let ready = prepare_port_state(index, &mut p, PreparePhase::Probe);
        ptr::addr_of_mut!((*current()).first_ports[index]).write(p);
        ready
    }
}
pub(super) unsafe fn reset_connected_ports() {
    unsafe {
        (*current()).connected_ports = 0;
        (*current()).enabled_ports = 0;
        (*current()).reset_ports = 0;
        for index in 0..usize::from((*current()).port_count_seen) {
            let mut p = ptr::addr_of!((*current()).first_ports[index]).read();
            prepare_port_state(index, &mut p, PreparePhase::Startup);
            if p.connected {
                (*current()).connected_ports += 1;
            }
            if p.enabled {
                (*current()).enabled_ports += 1;
            }
            ptr::addr_of_mut!((*current()).first_ports[index]).write(p);
        }
    }
}
pub(super) unsafe fn reclaim_runtime_for_port(index: usize, port: u8) -> bool {
    unsafe {
        if index >= 8 || !(*state()).runtimes[index].active {
            return true;
        }
        if defer_runtime_detach(ptr::addr_of_mut!((*state()).runtimes[index])) {
            return false;
        }
        persist_active_runtime();
        let slot = (*state()).runtimes[index].slot_id;
        if slot == 0 || (*state()).runtimes[index].port != port {
            return false;
        }
        if !super::xhci::disable_slot(slot) {
            return false;
        }
        super::core::remove(Text::literal(b"xhci"), port);
        release_runtime_by_slot(slot);
        (*current()).reclaimed_slots = (*current()).reclaimed_slots.wrapping_add(1);
        true
    }
}
pub(super) unsafe fn service_pending_port_change(port: u8) -> Outcome {
    unsafe {
        if port == 0 || port > (*current()).port_count_seen || usize::from(port) > MAX_PORTS {
            super::xhci::record_port_service_failure();
            return Outcome::default();
        }
        let index = usize::from(port - 1);
        let mut snapshot = ptr::addr_of!((*current()).first_ports[index]).read();
        refresh_port_status(&mut snapshot, index);
        store_port_snapshot(index, snapshot);
        let runtime = runtime_index_for_port(port);
        let action = decide_port_action(
            runtime.is_some(),
            snapshot.connected,
            snapshot.change_bits & PORTSC_CSC != 0,
        );
        let mut changed = false;
        if matches!(action, PortAction::Remove | PortAction::Replace)
            && let Some(runtime) = runtime
        {
            if !reclaim_runtime_for_port(runtime, port) {
                super::xhci::record_port_service_failure();
                (*current()).reason = Text::literal(b"xHCI hotplug reclaim failed");
                return Outcome {
                    retry: true,
                    ..Outcome::default()
                };
            }
            (*current()).port_disconnects = (*current()).port_disconnects.wrapping_add(1);
            changed = true;
        }
        if matches!(action, PortAction::Acknowledge | PortAction::Remove) {
            acknowledge_port_changes(index);
            return Outcome {
                catalog_changed: changed,
                ..Outcome::default()
            };
        }
        let mut prepared = ptr::addr_of!((*current()).first_ports[index]).read();
        prepared.debounce_ok = false;
        prepared.reset_attempted = false;
        prepared.reset_ok = false;
        prepared.reset_reason = Text::literal(b"hotplug-not-started");
        if !prepare_port_state(index, &mut prepared, PreparePhase::Startup) {
            store_port_snapshot(index, prepared);
            super::xhci::record_port_service_failure();
            (*current()).reason = Text::literal(b"xHCI hotplug port not ready");
            return Outcome {
                catalog_changed: changed,
                ..Outcome::default()
            };
        }
        store_port_snapshot(index, prepared);
        if !super::xhci::probe_port_device(index) || !(*current()).get_config_ok {
            super::xhci::disable_current_slot_for_scan();
            acknowledge_port_changes(index);
            super::xhci::record_port_service_failure();
            (*current()).reason = Text::literal(b"xHCI hotplug enumeration failed");
            return Outcome {
                catalog_changed: changed,
                ..Outcome::default()
            };
        }
        super::xhci::publish_current_core_device();
        persist_active_runtime();
        acknowledge_port_changes(index);
        Outcome {
            catalog_changed: true,
            ..Outcome::default()
        }
    }
}
