//! Original IRQ/IPI ordering with direct native scheduler/timer/TLB owners.
//! Saved IF follows the exact interrupted stack; no RAII runtime token or
//! state reference survives a physical task switch.
use super::{interrupt_frame::InterruptFrame, ioapic, lapic, percpu, pic, runtime};
use crate::{platform::timer, sched::scheduler};

fn event(operation: u8, a: u64, b: u64) -> u64 {
    unsafe { irq_event(operation, a, b) }
}
fn eoi() {
    lapic::r4os_rust_lapic_control(8, 0, 0);
}
/// # Safety
/// isr.S retains the complete frame and FPU image on the exact interrupted task
/// stack. Local runtime/IF follows that stack through physical preemption.
pub unsafe fn dispatch(frame: *const InterruptFrame) {
    if super::machine_check::terminal_active() {
        crate::boot::fatal::halt();
    }
    let saved = runtime::save_runtime();
    let vector = unsafe { core::ptr::addr_of!((*frame).vector).read() };
    if !(0x20..0x40).contains(&vector) {
        runtime::restore_runtime(saved);
        return;
    }
    let irq = (vector - 0x20) as u8;
    if irq == 0 && !lapic::on_timer_irq() {
        eoi();
        runtime::restore_runtime(saved);
        return;
    }
    let rip = unsafe { core::ptr::addr_of!((*frame).rip).read() };
    let preemptible = event(0, rip, 0) != 0;
    if runtime::current_cpu() != 0 && irq == 0 {
        let preempt = scheduler::on_tick(timer::tick_count(), preemptible, true);
        eoi();
        if preempt {
            scheduler::preempt_from_irq();
        }
        runtime::restore_runtime(saved);
        return;
    }
    let mut wake_checkpoint = scheduler::wakeup_checkpoint();
    let mut preempt = false;
    match irq {
        0 => {
            preempt =
                scheduler::on_tick(timer::r4os_rust_timer_control(4, 0, 0), preemptible, false);
            wake_checkpoint = scheduler::wakeup_checkpoint();
        }
        1 | 12 => {
            event(5, u64::from(irq), 0);
        }
        _ => {}
    }
    event(6, u64::from(irq), 0);
    if !ioapic::r4os_rust_ioapic_control(0, 0, false) || irq < 16 {
        pic::r4os_rust_pic_control(4, irq);
    }
    eoi();
    if !preempt && (irq != 0 || scheduler::wakeup_checkpoint() != wake_checkpoint) {
        preempt = scheduler::preempt_pending(preemptible);
    }
    if preempt {
        scheduler::preempt_from_irq();
    }
    runtime::restore_runtime(saved);
}
/// # Safety
/// Same exact assembly frame contract. TLB completion remains independent of
/// runtime admission while another CPU can retain the page-table owner.
pub unsafe fn dispatch_ipi(frame: *const InterruptFrame) {
    if super::machine_check::terminal_active() {
        crate::boot::fatal::halt();
    }
    match unsafe { core::ptr::addr_of!((*frame).vector).read() } {
        0xf0 => {
            eoi();
            let rip = unsafe { core::ptr::addr_of!((*frame).rip).read() };
            scheduler::on_reschedule(event(0, rip, 0) != 0);
        }
        0xf1 => {
            eoi();
            // A stale stop vector cannot stop a newly qualified execution
            // epoch. Logical park uses its separately authenticated receipts.
            if super::cpu_lifecycle::is_terminal()
                && percpu::r4os_rust_percpu_control(4, percpu::current_index(), 5, 6) != 0
            {
                runtime::halt_forever();
            }
        }
        0xf2 => {
            super::tlb::handle_ipi();
            eoi();
        }
        _ => eoi(),
    }
}

unsafe fn irq_event(operation: u8, a: u64, b: u64) -> u64 {
    let _ = b;
    match operation {
        0 => u64::from(crate::program::peer_lifetime::r4os_rust_program_peer_preemptible(a)),
        5 => {
            crate::input::i8042::on_irq(a.try_into().unwrap());
            0
        }
        6 => {
            unsafe {
                crate::driver::irq_router::r4os_rust_irq_router_dispatch(a.try_into().unwrap());
            }
            0
        }
        _ => panic!("Invalid IRQ owner operation"),
    }
}
