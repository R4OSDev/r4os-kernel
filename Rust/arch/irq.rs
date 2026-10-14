//! Original IRQ/IPI dispatch order. Scheduler, timer and device owners supply
//! fixed scalar C operations until their waves; no borrowed frame crosses them.
use super::{interrupt_frame::InterruptFrame, ioapic, lapic, pic, runtime};
unsafe extern "C" {
    fn r4os_zig_irq_event(operation: u8, a: u64, b: u64) -> u64;
}
fn event(operation: u8, a: u64, b: u64) -> u64 {
    unsafe { r4os_zig_irq_event(operation, a, b) }
}
fn eoi() {
    lapic::r4os_rust_lapic_control(8, 0, 0);
}
/// # Safety
/// isr.S owns the full resident frame on this exact interrupted task stack,
/// including its FPU state. The frame remains valid across an IRQ preemption.
pub unsafe fn dispatch(frame: *const InterruptFrame) {
    let _admission = runtime::RuntimeGuard::acquire();
    let vector = unsafe { core::ptr::addr_of!((*frame).vector).read() };
    if !(0x20..0x40).contains(&vector) {
        return;
    }
    let irq = (vector - 0x20) as u8;
    let rip = unsafe { core::ptr::addr_of!((*frame).rip).read() };
    let preemptible = event(0, rip, 0) != 0;
    if runtime::current_cpu() != 0 && irq == 0 {
        let now = event(2, 0, 0);
        let preempt = event(1, now, u64::from(preemptible)) != 0;
        eoi();
        if preempt {
            event(8, 0, 0);
        }
        return;
    }
    let mut wake_checkpoint = event(3, 0, 0);
    let mut preempt = false;
    match irq {
        0 => {
            preempt = event(4, u64::from(preemptible), 0) != 0;
            wake_checkpoint = event(3, 0, 0);
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
    if !preempt && (irq != 0 || event(3, 0, 0) != wake_checkpoint) {
        preempt = event(7, u64::from(preemptible), 0) != 0;
    }
    if preempt {
        event(8, 0, 0);
    }
}
/// # Safety
/// Same resident assembly-frame/FPU contract. High-priority TLB completion
/// remains independent of runtime admission even while another CPU holds it.
pub unsafe fn dispatch_ipi(frame: *const InterruptFrame) {
    match unsafe { core::ptr::addr_of!((*frame).vector).read() } {
        0xf0 => {
            eoi();
            let rip = unsafe { core::ptr::addr_of!((*frame).rip).read() };
            let preemptible = event(0, rip, 0);
            event(9, preemptible, 0);
        }
        0xf1 => {
            eoi();
            event(10, 0, 0);
            runtime::halt_forever();
        }
        0xf2 => {
            event(11, 0, 0);
            eoi();
        }
        _ => eoi(),
    }
}
