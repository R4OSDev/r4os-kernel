//! Original port debounce/power/reset and hotplug lifecycle. Mutable port
//! snapshots are caller-local copies, never Rust borrows into live status over
//! a wait. Detached transactions retain DMA until their exact terminal exit.
use super::xhci_state::Context;
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
pub(super) fn port_link_state(_ctx: Context, portsc: u32) -> u32 {
    (portsc & PORTSC_PLS_MASK) >> 5
}

fn phase_reason(
    _ctx: Context,
    phase: PreparePhase,
    startup: &'static [u8],
    probe: &'static [u8],
) -> Text {
    Text::literal(match phase {
        PreparePhase::Startup => startup,
        PreparePhase::Probe => probe,
    })
}
pub(super) unsafe fn refresh_port_status(ctx: Context, p: &mut PortStatus, index: usize) {
    unsafe {
        let base = OP_PORT_BASE + index as u64 * OP_PORT_STRIDE;
        p.portsc = read_op32(ctx, base);
        p.portpmsc = read_op32(ctx, base + 4);
        p.portli = read_op32(ctx, base + 8);
        p.porthlpmc = read_op32(ctx, base + 12);
        p.change_bits = p.portsc & PORTSC_CHANGE_MASK;
        p.link_state = port_link_state(ctx, p.portsc) as u8;
        p.connected = p.portsc & PORTSC_CCS != 0;
        p.enabled = p.portsc & PORTSC_PED != 0;
        p.powered = p.portsc & PORTSC_PP != 0;
    }
}
pub(super) unsafe fn clear_port_changes(ctx: Context, index: usize) -> u32 {
    unsafe {
        let base = OP_PORT_BASE + index as u64 * OP_PORT_STRIDE;
        let portsc = read_op32(ctx, base);
        let changes = portsc & PORTSC_CHANGE_MASK;
        if changes != 0 {
            write_op32(ctx, base, (portsc & PORTSC_WRITE_PRESERVE_MASK) | changes);
            (*current(ctx)).port_change_clears += 1;
        }
        changes
    }
}
pub(super) unsafe fn store_port_snapshot(ctx: Context, index: usize, snapshot: PortStatus) {
    unsafe {
        if index >= usize::from((*current(ctx)).port_count_seen) || index >= MAX_PORTS {
            return;
        }
        let previous = ptr::addr_of!((*current(ctx)).first_ports[index]).read();
        if previous.connected != snapshot.connected {
            if snapshot.connected {
                (*current(ctx)).connected_ports = (*current(ctx)).connected_ports.saturating_add(1);
            } else if (*current(ctx)).connected_ports > 0 {
                (*current(ctx)).connected_ports -= 1;
            }
        }
        if previous.enabled != snapshot.enabled {
            if snapshot.enabled {
                (*current(ctx)).enabled_ports = (*current(ctx)).enabled_ports.saturating_add(1);
            } else if (*current(ctx)).enabled_ports > 0 {
                (*current(ctx)).enabled_ports -= 1;
            }
        }
        ptr::addr_of_mut!((*current(ctx)).first_ports[index]).write(snapshot);
    }
}
pub(super) unsafe fn refresh_port_snapshot_by_number(ctx: Context, port: u8) {
    unsafe {
        if port == 0 || port > (*current(ctx)).port_count_seen {
            return;
        }
        let index = usize::from(port - 1);
        let mut snapshot = ptr::addr_of!((*current(ctx)).first_ports[index]).read();
        refresh_port_status(ctx, &mut snapshot, index);
        store_port_snapshot(ctx, index, snapshot);
    }
}
pub(super) unsafe fn acknowledge_port_changes(ctx: Context, index: usize) {
    unsafe {
        clear_port_changes(ctx, index);
        let mut p = ptr::addr_of!((*current(ctx)).first_ports[index]).read();
        refresh_port_status(ctx, &mut p, index);
        store_port_snapshot(ctx, index, p);
    }
}
pub(super) unsafe fn port_is_connected(ctx: Context, port: u8) -> bool {
    unsafe {
        for index in 0..usize::from((*current(ctx)).port_count_seen) {
            let p = ptr::addr_of!((*current(ctx)).first_ports[index]).read();
            if p.index == port {
                return p.connected;
            }
        }
        false
    }
}

pub(super) unsafe fn debounce_connected_port(
    ctx: Context,
    index: usize,
    p: &mut PortStatus,
) -> bool {
    unsafe {
        let base = OP_PORT_BASE + index as u64 * OP_PORT_STRIDE;
        let observed = PORTSC_CCS | PORTSC_PED | PORTSC_PLS_MASK | PORTSC_PR | PORTSC_WPR;
        let mut last = read_op32(ctx, base);
        let mut wait = Wait::begin(
            timing::XHCI_PORT_DEBOUNCE_TIMEOUT_MS,
            Text::literal(b"xhci-port-debounce"),
            0,
        );
        let mut stable = Deadline::begin(timing::XHCI_PORT_DEBOUNCE_STABLE_MS);
        while !wait.expired() {
            let now = read_op32(ctx, base);
            if now & PORTSC_CCS == 0 {
                stable.finish();
                wait.finish(false);
                refresh_port_status(ctx, p, index);
                return false;
            }
            if now & observed != last & observed {
                stable.finish();
                stable = Deadline::begin(timing::XHCI_PORT_DEBOUNCE_STABLE_MS);
                last = now;
            } else if stable.expired_any() {
                stable.finish();
                wait.finish(true);
                refresh_port_status(ctx, p, index);
                p.debounce_ok = true;
                return true;
            }
            if super::xhci::irq_registered(ctx) && !scheduler::current().is_null() {
                if super::xhci::wait_event_irq(ctx, EVENT_IRQ_POLL_TICKS) {
                    super::xhci::record_irq_wakeup(ctx);
                } else {
                    super::xhci::record_poll_fallback(ctx);
                }
            } else {
                super::xhci::record_poll_fallback(ctx);
                wait.idle();
            }
        }
        stable.finish();
        wait.finish(false);
        refresh_port_status(ctx, p, index);
        p.debounce_ok = false;
        false
    }
}
pub(super) unsafe fn ensure_port_power(ctx: Context, index: usize, p: &mut PortStatus) -> bool {
    unsafe {
        if p.powered {
            return true;
        }
        let base = OP_PORT_BASE + index as u64 * OP_PORT_STRIDE;
        (*current(ctx)).port_power_requests += 1;
        let before = read_op32(ctx, base);
        write_op32(
            ctx,
            base,
            (before & PORTSC_WRITE_PRESERVE_MASK) | PORTSC_PP | (before & PORTSC_CHANGE_MASK),
        );
        let mut wait = Wait::begin(
            timing::XHCI_PORT_POWER_TIMEOUT_MS,
            Text::literal(b"xhci-port-power"),
            0,
        );
        while !wait.expired() {
            refresh_port_status(ctx, p, index);
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
        (*current(ctx)).timeouts += 1;
        false
    }
}
pub(super) unsafe fn reset_port(ctx: Context, index: u8) -> bool {
    unsafe {
        if let Some(runtime) = runtime_index_for_port(ctx, index.saturating_add(1))
            && (*state(ctx)).runtimes[runtime].transaction_active
        {
            return false;
        }
        let base = OP_PORT_BASE + u64::from(index) * OP_PORT_STRIDE;
        let before = read_op32(ctx, base);
        clear_port_changes(ctx, usize::from(index));
        write_op32(ctx, base, (before & PORTSC_WRITE_PRESERVE_MASK) | PORTSC_PR);
        let mut wait = Wait::begin(
            timing::XHCI_PORT_RESET_TIMEOUT_MS,
            Text::literal(b"xhci-port-reset"),
            0,
        );
        while !wait.expired() {
            let portsc = read_op32(ctx, base);
            if portsc & PORTSC_PR == 0 && (portsc & PORTSC_PED != 0 || portsc & PORTSC_CCS == 0) {
                clear_port_changes(ctx, usize::from(index));
                wait.finish(true);
                return true;
            }
            wait.idle();
        }
        wait.finish(false);
        (*current(ctx)).timeouts += 1;
        false
    }
}
pub(super) unsafe fn warm_reset_port(ctx: Context, index: u8) -> bool {
    unsafe {
        if let Some(runtime) = runtime_index_for_port(ctx, index.saturating_add(1))
            && (*state(ctx)).runtimes[runtime].transaction_active
        {
            return false;
        }
        let base = OP_PORT_BASE + u64::from(index) * OP_PORT_STRIDE;
        let before = read_op32(ctx, base);
        clear_port_changes(ctx, usize::from(index));
        write_op32(
            ctx,
            base,
            (before & PORTSC_WRITE_PRESERVE_MASK) | PORTSC_WPR,
        );
        let mut wait = Wait::begin(
            timing::XHCI_PORT_WARM_RESET_TIMEOUT_MS,
            Text::literal(b"xhci-port-warm-reset"),
            0,
        );
        while !wait.expired() {
            let portsc = read_op32(ctx, base);
            if portsc & PORTSC_WPR == 0 {
                clear_port_changes(ctx, usize::from(index));
                let ready = portsc & PORTSC_CCS == 0
                    || (portsc & PORTSC_PED != 0 && port_link_state(ctx, portsc) == PORTSC_PLS_U0);
                wait.finish(ready);
                return ready;
            }
            wait.idle();
        }
        wait.finish(false);
        (*current(ctx)).timeouts += 1;
        false
    }
}
pub(super) unsafe fn wait_port_u0(ctx: Context, index: u8, p: &mut PortStatus) -> bool {
    unsafe {
        (*current(ctx)).port_u0_waits += 1;
        let mut wait = Wait::begin(
            timing::XHCI_PORT_U0_TIMEOUT_MS,
            Text::literal(b"xhci-port-u0"),
            0,
        );
        while !wait.expired() {
            refresh_port_status(ctx, p, usize::from(index));
            if p.portsc & PORTSC_CCS == 0 {
                wait.finish(false);
                return false;
            }
            if p.portsc & PORTSC_PED != 0 && port_link_state(ctx, p.portsc) == PORTSC_PLS_U0 {
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
    ctx: Context,
    index: usize,
    p: &mut PortStatus,
    phase: PreparePhase,
) -> bool {
    unsafe {
        refresh_port_status(ctx, p, index);
        let debounced = p.debounce_ok;
        let changes = clear_port_changes(ctx, index);
        if changes != 0 {
            p.change_bits |= changes;
            refresh_port_status(ctx, p, index);
        }
        if !p.connected {
            p.debounce_ok = false;
            p.reset_reason = Text::literal(b"no-device");
            return false;
        }
        p.debounce_ok = debounced && changes & PORTSC_CSC == 0;
        if !p.debounce_ok && !debounce_connected_port(ctx, index, p) {
            p.reset_reason =
                phase_reason(ctx, phase, b"debounce-timeout", b"probe-debounce-timeout");
            (*current(ctx)).port_debounce_failures += 1;
            return false;
        }
        if !ensure_port_power(ctx, index, p) {
            p.reset_reason = phase_reason(ctx, phase, b"power-timeout", b"probe-power-timeout");
            return false;
        }
        let force = phase == PreparePhase::Startup && !p.reset_attempted;
        if !p.enabled || force {
            p.reset_attempted = true;
            p.reset_ok = reset_port(ctx, u8::try_from(index).expect("USB port index"));
            p.reset_reason = phase_reason(
                ctx,
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
                (*current(ctx)).reset_ports += 1;
            }
            refresh_port_status(ctx, p, index);
            if p.reset_ok && p.connected && p.enabled {
                super::wait::milliseconds(
                    timing::PORT_RESET_RECOVERY_MS,
                    Text::literal(b"xhci-port-reset-recovery"),
                    0,
                );
                refresh_port_status(ctx, p, index);
            }
        }
        if p.enabled && port_link_state(ctx, p.portsc) != PORTSC_PLS_U0 {
            if wait_port_u0(ctx, u8::try_from(index).expect("USB port index"), p) {
                p.reset_reason = phase_reason(ctx, phase, b"waited-u0", b"probe-waited-u0");
            } else {
                p.reset_attempted = true;
                p.reset_ok = warm_reset_port(ctx, u8::try_from(index).expect("USB port index"));
                (*current(ctx)).port_warm_resets += 1;
                p.reset_reason = phase_reason(
                    ctx,
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
                    (*current(ctx)).reset_ports += 1;
                }
                refresh_port_status(ctx, p, index);
                if p.connected
                    && p.enabled
                    && port_link_state(ctx, p.portsc) != PORTSC_PLS_U0
                    && wait_port_u0(ctx, u8::try_from(index).expect("USB port index"), p)
                {
                    p.reset_reason =
                        phase_reason(ctx, phase, b"warm-reset-waited-u0", b"probe-warm-waited-u0");
                }
            }
        } else if p.connected && p.enabled && (p.reset_reason.len == 0 || !p.reset_attempted) {
            p.reset_reason = phase_reason(ctx, phase, b"already-enabled", b"probe-already-enabled");
        }
        clear_port_changes(ctx, index);
        refresh_port_status(ctx, p, index);
        p.connected && p.enabled && port_link_state(ctx, p.portsc) == PORTSC_PLS_U0
    }
}
pub(super) unsafe fn prepare_port_for_probe(ctx: Context, index: usize) -> bool {
    unsafe {
        if index >= usize::from((*current(ctx)).port_count_seen) {
            return false;
        }
        let mut p = ptr::addr_of!((*current(ctx)).first_ports[index]).read();
        let ready = prepare_port_state(ctx, index, &mut p, PreparePhase::Probe);
        ptr::addr_of_mut!((*current(ctx)).first_ports[index]).write(p);
        ready
    }
}
pub(super) unsafe fn reset_connected_ports(ctx: Context) {
    unsafe {
        (*current(ctx)).connected_ports = 0;
        (*current(ctx)).enabled_ports = 0;
        (*current(ctx)).reset_ports = 0;
        for index in 0..usize::from((*current(ctx)).port_count_seen) {
            let mut p = ptr::addr_of!((*current(ctx)).first_ports[index]).read();
            prepare_port_state(ctx, index, &mut p, PreparePhase::Startup);
            if p.connected {
                (*current(ctx)).connected_ports += 1;
            }
            if p.enabled {
                (*current(ctx)).enabled_ports += 1;
            }
            ptr::addr_of_mut!((*current(ctx)).first_ports[index]).write(p);
        }
    }
}
pub(super) unsafe fn reclaim_runtime_for_port(ctx: Context, index: usize, port: u8) -> bool {
    unsafe {
        if index >= 8 || !(*state(ctx)).runtimes[index].active {
            return true;
        }
        if defer_runtime_detach(ctx, ptr::addr_of_mut!((*state(ctx)).runtimes[index])) {
            return false;
        }
        persist_active_runtime(ctx);
        let slot = (*state(ctx)).runtimes[index].slot_id;
        if slot == 0 || (*state(ctx)).runtimes[index].port != port {
            return false;
        }
        if !super::xhci::disable_slot(ctx, slot) {
            return false;
        }
        super::core::remove(ctx.name(), port);
        release_runtime_by_slot(ctx, slot);
        (*current(ctx)).reclaimed_slots = (*current(ctx)).reclaimed_slots.wrapping_add(1);
        true
    }
}
pub(super) unsafe fn service_pending_port_change(ctx: Context, port: u8) -> Outcome {
    unsafe {
        if port == 0 || port > (*current(ctx)).port_count_seen || usize::from(port) > MAX_PORTS {
            super::xhci::record_port_service_failure(ctx);
            return Outcome::default();
        }
        let index = usize::from(port - 1);
        let mut snapshot = ptr::addr_of!((*current(ctx)).first_ports[index]).read();
        refresh_port_status(ctx, &mut snapshot, index);
        store_port_snapshot(ctx, index, snapshot);
        let runtime = runtime_index_for_port(ctx, port);
        let action = decide_port_action(
            runtime.is_some(),
            snapshot.connected,
            snapshot.change_bits & PORTSC_CSC != 0,
        );
        let mut changed = false;
        if matches!(action, PortAction::Remove | PortAction::Replace)
            && let Some(runtime) = runtime
        {
            if !reclaim_runtime_for_port(ctx, runtime, port) {
                super::xhci::record_port_service_failure(ctx);
                (*current(ctx)).reason = Text::literal(b"xHCI hotplug reclaim failed");
                return Outcome {
                    retry: true,
                    ..Outcome::default()
                };
            }
            (*current(ctx)).port_disconnects = (*current(ctx)).port_disconnects.wrapping_add(1);
            changed = true;
        }
        if matches!(action, PortAction::Acknowledge | PortAction::Remove) {
            acknowledge_port_changes(ctx, index);
            return Outcome {
                catalog_changed: changed,
                ..Outcome::default()
            };
        }
        let mut prepared = ptr::addr_of!((*current(ctx)).first_ports[index]).read();
        prepared.debounce_ok = false;
        prepared.reset_attempted = false;
        prepared.reset_ok = false;
        prepared.reset_reason = Text::literal(b"hotplug-not-started");
        if !prepare_port_state(ctx, index, &mut prepared, PreparePhase::Startup) {
            store_port_snapshot(ctx, index, prepared);
            super::xhci::record_port_service_failure(ctx);
            (*current(ctx)).reason = Text::literal(b"xHCI hotplug port not ready");
            return Outcome {
                catalog_changed: changed,
                ..Outcome::default()
            };
        }
        store_port_snapshot(ctx, index, prepared);
        if !super::xhci::probe_port_device(ctx, index) || !(*current(ctx)).get_config_ok {
            super::xhci::disable_current_slot_for_scan(ctx);
            acknowledge_port_changes(ctx, index);
            super::xhci::record_port_service_failure(ctx);
            (*current(ctx)).reason = Text::literal(b"xHCI hotplug enumeration failed");
            return Outcome {
                catalog_changed: changed,
                ..Outcome::default()
            };
        }
        super::xhci::publish_current_core_device(ctx);
        persist_active_runtime(ctx);
        acknowledge_port_changes(ctx, index);
        Outcome {
            catalog_changed: true,
            ..Outcome::default()
        }
    }
}
