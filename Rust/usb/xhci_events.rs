//! Sole event-ring consumer and exact live-TD routing. Interrupts only wake
//! workers; they never consume controller state or acknowledge a transfer.
use super::xhci_state::Context;
use super::{
    event_router::{self, Event, Match},
    ring_cycle, timing,
    wait_types::{Deadline, Wait},
    xhci::*,
    xhci_constants::*,
    xhci_diag as diag,
    xhci_state::{current, state},
    xhci_types::{DeviceRuntime, Trb},
};
use crate::{boot::foundation::puts, sched::sync_types::Text};
use core::ptr;
#[derive(Clone, Copy)]
enum TimeoutClock {
    Ticks,
    Hpet,
    Tsc,
    TscFallback,
    RecoveryBudget,
    CpuGuard,
}
impl TimeoutClock {
    fn label(self) -> &'static [u8] {
        match self {
            Self::Ticks => b"ticks",
            Self::Hpet => b"hpet",
            Self::Tsc => b"tsc",
            Self::TscFallback => b"tsc-fallback",
            Self::RecoveryBudget => b"recovery-budget",
            Self::CpuGuard => b"cpu-guard",
        }
    }
}
pub(super) unsafe fn current_interrupt_owner_match(ctx: Context) -> Option<Match> {
    unsafe {
        if !(*current(ctx)).interrupt_pending
            || (*current(ctx)).interrupt_pending_trb_phys == 0
            || (*current(ctx)).addressed_slot_id == 0
            || (*current(ctx)).interrupt_endpoint_id == 0
        {
            return None;
        }
        Some(Match::transfer(
            (*current(ctx)).addressed_slot_id,
            (*current(ctx)).interrupt_endpoint_id,
            (*current(ctx)).interrupt_pending_trb_phys,
        ))
    }
}
fn runtime_interrupt_owner_match(_ctx: Context, rt: DeviceRuntime) -> Option<Match> {
    if !rt.active
        || !rt.interrupt_pending
        || rt.interrupt_pending_trb_phys == 0
        || rt.slot_id == 0
        || rt.interrupt_endpoint_id == 0
    {
        return None;
    }
    Some(Match::transfer(
        rt.slot_id,
        rt.interrupt_endpoint_id,
        rt.interrupt_pending_trb_phys,
    ))
}
unsafe fn event_has_live_owner(ctx: Context, event: Event) -> bool {
    unsafe {
        if (*state(ctx))
            .transfer_objects
            .matching_handle(event)
            .is_some()
        {
            return true;
        }
        if (*state(ctx))
            .active_command
            .is_some_and(|m| event_router::matches(event, m))
        {
            return true;
        }
        if current_interrupt_owner_match(ctx).is_some_and(|m| event_router::matches(event, m)) {
            return true;
        }
        for i in 0..8 {
            if (*state(ctx)).active_runtime_index == Some(i) {
                continue;
            }
            if runtime_interrupt_owner_match(ctx, ptr::addr_of!((*state(ctx)).runtimes[i]).read())
                .is_some_and(|m| event_router::matches(event, m))
            {
                return true;
            }
        }
        false
    }
}
unsafe fn clear_overflowed_interrupt_owner(ctx: Context, event: Event) -> bool {
    unsafe {
        if let Some(handle) = (*state(ctx)).transfer_objects.matching_handle(event) {
            (*state(ctx)).transfer_objects.cancel(handle);
            (*state(ctx)).transfer_objects.release(handle);
        }
        if current_interrupt_owner_match(ctx).is_some_and(|m| event_router::matches(event, m)) {
            (*current(ctx)).interrupt_pending = false;
            (*current(ctx)).interrupt_pending_trb_phys = 0;
            (*current(ctx)).interrupt_transfer_handle = 0;
            (*current(ctx)).interrupt_pending_streak = 0;
            return true;
        }
        for i in 0..8 {
            if (*state(ctx)).active_runtime_index == Some(i) {
                continue;
            }
            let rt = ptr::addr_of_mut!((*state(ctx)).runtimes[i]);
            if !runtime_interrupt_owner_match(ctx, rt.read())
                .is_some_and(|m| event_router::matches(event, m))
            {
                continue;
            }
            (*rt).interrupt_pending = false;
            (*rt).interrupt_pending_trb_phys = 0;
            (*rt).interrupt_transfer_handle = 0;
            (*rt).interrupt_pending_streak = 0;
            return true;
        }
        false
    }
}
pub(super) unsafe fn route_foreign_event(ctx: Context, event: Event) {
    unsafe {
        if (*state(ctx))
            .pending_port_changes
            .route(event, (*current(ctx)).port_count_seen)
        {
            return;
        }
        if event_has_live_owner(ctx, event) {
            if (*state(ctx)).deferred_events.enqueue(event) {
                super::xhci_async::signal_completion(ctx, event);
                notify_input(ctx, event);
                return;
            }
            clear_overflowed_interrupt_owner(ctx, event);
        }
        record_stale_event(ctx, event);
        (*current(ctx)).stale_events += 1;
    }
}
pub(super) unsafe fn poll_matching_event(ctx: Context, expected: Match) -> Option<Event> {
    unsafe {
        if let Some(e) = (*state(ctx)).deferred_events.take(expected) {
            return Some(e);
        }
        drain_event_batch(ctx, expected)
    }
}
pub(super) unsafe fn drain_event_batch(ctx: Context, expected: Match) -> Option<Event> {
    unsafe {
        let mut matched = None;
        let mut processed = 0u16;
        for _ in 0..1024 {
            let Some(event) = poll_event(ctx) else {
                break;
            };
            processed = processed.wrapping_add(1);
            if matched.is_none() && event_router::matches(event, expected) {
                matched = Some(event);
            } else {
                route_foreign_event(ctx, event);
            }
        }
        commit_event_batch(ctx, processed);
        matched
    }
}
pub(super) unsafe fn wait_matching_event_within(
    ctx: Context,
    expected: Match,
    budget: Option<&Deadline>,
) -> Option<Event> {
    unsafe {
        (*current(ctx)).last_wait_cancelled = false;
        if let Some(e) = (*state(ctx)).deferred_events.take(expected) {
            ctx.asynchronous()
                .completion_fast
                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            return Some(e);
        }
        if let Some(e) = drain_event_batch(ctx, expected) {
            ctx.asynchronous()
                .completion_fast
                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            return Some(e);
        }
        let completion = super::xhci_async::CompletionWait::enroll(ctx, expected);
        let mut wait = Wait::begin(
            timing::XHCI_EVENT_TIMEOUT_MS,
            Text::literal(b"xhci-event"),
            0,
        );
        if wait.deadline.uses_tick_deadline() {
            (*current(ctx)).event_tick_deadline_waits =
                (*current(ctx)).event_tick_deadline_waits.wrapping_add(1);
        } else if wait.deadline.uses_hpet_deadline() {
            (*current(ctx)).event_hpet_deadline_waits =
                (*current(ctx)).event_hpet_deadline_waits.wrapping_add(1);
        } else if wait.deadline.uses_tsc_deadline() {
            (*current(ctx)).event_tsc_deadline_waits =
                (*current(ctx)).event_tsc_deadline_waits.wrapping_add(1);
        } else {
            (*current(ctx)).event_guard_waits = (*current(ctx)).event_guard_waits.wrapping_add(1);
        }
        let mut guard = 0u32;
        let clock = loop {
            if let Some(i) = (*state(ctx)).active_runtime_index {
                let rt = ptr::addr_of!((*state(ctx)).runtimes[i]).read();
                if super::xhci_owner::transaction_owned(ctx, rt) && rt.detach_pending {
                    (*current(ctx)).last_wait_cancelled = true;
                    wait.finish(false);
                    return None;
                }
            }
            if budget.is_some_and(Deadline::expired_any) {
                break TimeoutClock::RecoveryBudget;
            }
            if wait.deadline.expired() {
                break TimeoutClock::Ticks;
            }
            if wait.deadline.hpet_expired() {
                break TimeoutClock::Hpet;
            }
            if wait.deadline.tsc_expired() {
                break if wait.deadline.uses_fallback_tsc() {
                    TimeoutClock::TscFallback
                } else {
                    TimeoutClock::Tsc
                };
            }
            if let Some(e) = drain_event_batch(ctx, expected) {
                if wait.iterations == 0 {
                    completion.rechecked();
                }
                wait.finish(true);
                return Some(e);
            }
            if let Some(e) = (*state(ctx)).deferred_events.take(expected) {
                if wait.iterations == 0 {
                    completion.rechecked();
                }
                wait.finish(true);
                return Some(e);
            }
            if guard >= COMMAND_WAIT_GUARD {
                break TimeoutClock::CpuGuard;
            }
            idle_device_wait(ctx, &mut wait, completion.slot);
            guard = guard.wrapping_add(1);
        };
        let ticks = wait.deadline.elapsed_ticks();
        let hpet = wait.deadline.elapsed_hpet();
        let tsc = wait.deadline.elapsed_tsc();
        wait.finish(false);
        match clock {
            TimeoutClock::Ticks => {
                (*current(ctx)).event_tick_timeouts =
                    (*current(ctx)).event_tick_timeouts.wrapping_add(1)
            }
            TimeoutClock::Hpet => {
                (*current(ctx)).event_hpet_timeouts =
                    (*current(ctx)).event_hpet_timeouts.wrapping_add(1)
            }
            TimeoutClock::RecoveryBudget => {
                (*current(ctx)).event_recovery_budget_timeouts = (*current(ctx))
                    .event_recovery_budget_timeouts
                    .wrapping_add(1)
            }
            TimeoutClock::Tsc | TimeoutClock::TscFallback => {
                (*current(ctx)).event_tsc_timeouts =
                    (*current(ctx)).event_tsc_timeouts.wrapping_add(1)
            }
            TimeoutClock::CpuGuard => {
                (*current(ctx)).event_guard_timeouts =
                    (*current(ctx)).event_guard_timeouts.wrapping_add(1);
                (*current(ctx)).event_cpu_guard_timeouts =
                    (*current(ctx)).event_cpu_guard_timeouts.wrapping_add(1);
            }
        }
        let token = diag::begin();
        if expected.tag == 0 && token.valid() {
            close_last_sync_transfer_incident(ctx);
            (*state(ctx)).last_sync_transfer_incident = token;
        }
        timeout_diagnostic(ctx, expected, clock, ticks, hpet, tsc, guard);
        if expected.tag == 1 {
            diag::resolve(token);
        }
        None
    }
}
unsafe fn timeout_diagnostic(
    ctx: Context,
    expected: Match,
    clock: TimeoutClock,
    ticks: u64,
    hpet: u64,
    tsc: u64,
    guard: u32,
) {
    unsafe {
        // Screen and persistent console retain their original distinct sinks.
        diag::write(b"[XHCI] event timeout owner=");
        if expected.tag == 0 {
            diag::write(b"transfer slot=");
            diag::dec(expected.slot_id);
            diag::write(b" ep=");
            diag::dec(expected.endpoint_id);
            diag::write(b" trb=");
            diag::hex(expected.trb_phys[0]);
        } else {
            diag::write(b"command trb=");
            diag::hex(expected.trb_phys[0]);
        }
        diag::write(b" clock=");
        diag::write(clock.label());
        diag::write(b" elapsed_ticks=");
        diag::dec(ticks);
        diag::write(b" elapsed_hpet=");
        diag::dec(hpet);
        diag::write(b" elapsed_tsc=");
        diag::dec(tsc);
        diag::write(b" guard=");
        diag::dec(guard);
        diag::end();
        puts(b"[XHCI] event timeout owner=");
        if expected.tag == 0 {
            puts(b"transfer slot=");
            diag::console_dec(expected.slot_id);
            puts(b" ep=");
            diag::console_dec(expected.endpoint_id);
            puts(b" trb=0x");
            diag::console_hex(expected.trb_phys[0], 16);
        } else {
            puts(b"command trb=0x");
            diag::console_hex(expected.trb_phys[0], 16);
        }
        puts(b" clock=");
        puts(clock.label());
        puts(b" elapsed_ticks=");
        diag::console_dec(ticks);
        puts(b" elapsed_hpet=");
        diag::console_dec(hpet);
        puts(b" elapsed_tsc=");
        diag::console_dec(tsc);
        puts(b" guard=");
        diag::console_dec(guard);
        puts(b"\r\n");
        puts(b"[XHCI] hw usbcmd=0x");
        diag::console_hex(read_op32(ctx, OP_USBCMD), 8);
        puts(b" usbsts=0x");
        diag::console_hex(read_op32(ctx, OP_USBSTS), 8);
        puts(b" iman=0x");
        diag::console_hex(read_rt32(ctx, RT_IR0_IMAN), 8);
        puts(b" erdp=0x");
        diag::console_hex(read_rt64(ctx, RT_IR0_ERDP), 16);
        puts(b"\r\n");
        let next = if (*state(ctx)).event_ring_virt != 0 {
            volatile_read32(
                (*state(ctx)).event_ring_virt + u64::from((*current(ctx)).event_dequeue) * 16 + 12,
            )
        } else {
            0
        };
        diag::write(b"[XHCI] hw usbcmd=");
        diag::hex(read_op32(ctx, OP_USBCMD));
        diag::write(b" usbsts=");
        diag::hex(read_op32(ctx, OP_USBSTS));
        diag::write(b" iman=");
        diag::hex(read_rt32(ctx, RT_IR0_IMAN));
        diag::write(b" erdp=");
        diag::hex(read_rt64(ctx, RT_IR0_ERDP));
        diag::end();
        diag::write(b"[XHCI] ring deq=");
        diag::dec((*current(ctx)).event_dequeue);
        diag::write(b" cycle=");
        diag::dec((*current(ctx)).event_cycle);
        diag::write(b" next=");
        diag::hex(next);
        diag::write(b" events=");
        diag::dec((*current(ctx)).events);
        diag::write(b" last=");
        diag::dec((*current(ctx)).last_event_type);
        diag::write(b"/");
        diag::dec((*current(ctx)).last_event_code);
        diag::end();
        if expected.tag == 0 {
            diag::write(b"[XHCI] target epstate=");
            if let Some(s) = endpoint_state(ctx, expected.endpoint_id) {
                diag::dec(s as u8);
            } else {
                diag::write(b"unknown");
            }
            if (*current(ctx)).addressed_port != 0 {
                diag::write(b" portsc=");
                diag::hex(read_op32(
                    ctx,
                    OP_PORT_BASE + u64::from((*current(ctx)).addressed_port - 1) * OP_PORT_STRIDE,
                ));
            }
            diag::end();
        }
    }
}
pub(super) unsafe fn submit_command(
    ctx: Context,
    parameter: u64,
    status: u32,
    control: u32,
) -> Option<Event> {
    unsafe { submit_command_within(ctx, parameter, status, control, None) }
}
pub(super) unsafe fn submit_command_within(
    ctx: Context,
    parameter: u64,
    status: u32,
    control: u32,
    budget: Option<&Deadline>,
) -> Option<Event> {
    unsafe {
        if !(*current(ctx)).controller_running
            || (*state(ctx)).command_ring_virt == 0
            || (*state(ctx)).active_command.is_some()
            || budget.is_some_and(Deadline::expired_any)
        {
            return None;
        }
        let enqueue = (*current(ctx)).command_enqueue;
        if usize::from(enqueue) >= COMMAND_TRB_COUNT - 1 {
            (*current(ctx)).ring_full += 1;
            return None;
        }
        let ring = (*state(ctx)).command_ring_virt as *mut Trb;
        let physical = (*current(ctx)).command_ring_phys + u64::from(enqueue) * 16;
        let cycle = if (*current(ctx)).command_cycle != 0 {
            TRB_CYCLE
        } else {
            0
        };
        ring.add(usize::from(enqueue)).write(Trb {
            parameter,
            status,
            control: control | cycle,
        });
        dma_fence();
        (*current(ctx)).last_command_type = ((control >> TRB_TYPE_SHIFT) & 0x3f) as u8;
        (*current(ctx)).commands += 1;
        let advance =
            ring_cycle::after_submission(enqueue, (*current(ctx)).command_cycle, COMMAND_TRB_COUNT);
        if advance.wrapped {
            update_link_trb(
                ctx,
                (*state(ctx)).command_ring_virt,
                COMMAND_TRB_COUNT,
                advance.link_cycle,
                false,
            );
            (*current(ctx)).command_ring_wraps += 1;
        }
        (*current(ctx)).command_enqueue = advance.next_enqueue;
        (*current(ctx)).command_cycle = advance.next_producer_cycle;
        let expected = Match::command(physical);
        (*state(ctx)).active_command = Some(expected);
        write_doorbell(ctx, 0, 0);
        let result = wait_matching_event_within(ctx, expected, budget);
        (*state(ctx)).active_command = None;
        if result.is_none() {
            if !(*current(ctx)).last_wait_cancelled {
                (*current(ctx)).timeouts += 1;
            }
            (*current(ctx)).reason = Text::literal(if (*current(ctx)).last_wait_cancelled {
                b"USB device detached during command"
            } else {
                b"timeout waiting for xHCI command completion"
            });
        }
        result
    }
}
pub(super) unsafe fn wait_transfer_completion_within(
    ctx: Context,
    expected: Match,
    budget: Option<&Deadline>,
) -> Option<Event> {
    unsafe {
        let result = wait_matching_event_within(ctx, expected, budget);
        if result.is_none() {
            if !(*current(ctx)).last_wait_cancelled {
                (*current(ctx)).timeouts += 1;
            }
            (*current(ctx)).reason = Text::literal(if (*current(ctx)).last_wait_cancelled {
                b"USB device detached during transfer"
            } else {
                b"timeout waiting for xHCI transfer event"
            });
        }
        result
    }
}
pub(super) unsafe fn poll_event(ctx: Context) -> Option<Event> {
    unsafe {
        let addr = (*state(ctx)).event_ring_virt + u64::from((*current(ctx)).event_dequeue) * 16;
        let expected = if (*current(ctx)).event_cycle != 0 {
            TRB_CYCLE
        } else {
            0
        };
        if volatile_read32(addr + 12) & TRB_CYCLE != expected {
            return None;
        }
        dma_fence();
        let parameter = volatile_read64(addr);
        let status = volatile_read32(addr + 8);
        let control = volatile_read32(addr + 12);
        if control & TRB_CYCLE != expected {
            return None;
        }
        (*current(ctx)).events += 1;
        (*current(ctx)).event_dequeue += 1;
        if usize::from((*current(ctx)).event_dequeue) >= EVENT_TRB_COUNT {
            (*current(ctx)).event_dequeue = 0;
            (*current(ctx)).event_cycle = u8::from((*current(ctx)).event_cycle == 0);
            (*current(ctx)).event_ring_wraps += 1;
        }
        let event_type = ((control >> TRB_TYPE_SHIFT) & 0x3f) as u8;
        let code = (status >> 24) as u8;
        if code == COMPLETION_EVENT_RING_FULL_ERROR {
            (*current(ctx)).ring_full += 1;
        }
        if u32::from(event_type) == TRB_TYPE_TRANSFER_EVENT {
            (*current(ctx)).transfer_events += 1;
        }
        let out = Event {
            event_type,
            code,
            slot_id: (control >> 24) as u8,
            endpoint_id: ((control >> 16) & 31) as u8,
            parameter,
            length: status & 0x00ff_ffff,
            control,
        };
        (*current(ctx)).last_event_type = out.event_type;
        (*current(ctx)).last_event_code = out.code;
        (*current(ctx)).last_event_slot = out.slot_id;
        (*current(ctx)).last_event_endpoint = out.endpoint_id;
        (*current(ctx)).last_event_parameter = out.parameter;
        (*current(ctx)).last_event_length = out.length;
        Some(out)
    }
}
pub(super) unsafe fn commit_event_batch(ctx: Context, processed: u16) {
    unsafe {
        if processed == 0 {
            return;
        }
        (*current(ctx)).event_max_batch = (*current(ctx)).event_max_batch.max(processed);
        dma_fence();
        if read_op32(ctx, OP_USBSTS) & USBSTS_EINT != 0 {
            write_op32(ctx, OP_USBSTS, USBSTS_EINT);
            read_op32(ctx, OP_USBSTS);
        }
        let iman = read_rt32(ctx, RT_IR0_IMAN);
        if iman & IMAN_IP != 0 {
            write_rt32(ctx, RT_IR0_IMAN, iman | IMAN_IP);
            read_rt32(ctx, RT_IR0_IMAN);
        }
        let dequeue =
            (*current(ctx)).event_ring_phys + u64::from((*current(ctx)).event_dequeue) * 16;
        write_rt64(ctx, RT_IR0_ERDP, dequeue | ERDP_EHB);
        read_rt64(ctx, RT_IR0_ERDP);
        (*current(ctx)).event_erdp_commits = (*current(ctx)).event_erdp_commits.wrapping_add(1);
        rearm_event_irq(ctx);
    }
}
unsafe fn record_stale_event(ctx: Context, event: Event) {
    unsafe {
        (*current(ctx)).last_stale_event_type = event.event_type;
        (*current(ctx)).last_stale_event_code = event.code;
        (*current(ctx)).last_stale_event_slot = event.slot_id;
        (*current(ctx)).last_stale_event_endpoint = event.endpoint_id;
    }
}

// Publication to the exact report mailbox precedes this wake. A class
// subscription is separate from generic interrupt transfers and retained by
// runtime generation; slot reuse cannot notify an old/replacement binding.
unsafe fn notify_input(ctx: Context, event: Event) {
    unsafe {
        let s = state(ctx);
        let Some(hook) = (*s).input_hook else {
            return;
        };
        for i in 0..8 {
            let rt = ptr::addr_of!((*s).runtimes[i]).read();
            if !rt.active
                || rt.detach_pending
                || (*s).input_generations[i] != rt.generation
                || (*s).input_endpoints[i] != event.endpoint_id
            {
                continue;
            }
            let expected = if (*s).active_runtime_index == Some(i) {
                current_interrupt_owner_match(ctx)
            } else {
                runtime_interrupt_owner_match(ctx, rt)
            };
            if expected.is_some_and(|m| event_router::matches(event, m)) {
                hook();
                return;
            }
        }
    }
}
