//! Explicit KernelCheck-only cross-CPU test of the real router owner. The
//! resident software vector is not a device IRQ or a native hardware receipt.
use super::helpers::*;
use crate::{
    arch::{percpu, runtime, tlb},
    driver::irq_router,
    platform::{monotonic, timer},
    sched::{kernel_check, task::task_types::Role},
};
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, AtomicU32, Ordering};
static LINE: AtomicU8 = AtomicU8::new(0);
static ENTERED: AtomicBool = AtomicBool::new(false);
static DONE: AtomicBool = AtomicBool::new(false);
static OBSERVED: AtomicI32 = AtomicI32::new(-99);
static HITS: AtomicU32 = AtomicU32::new(0);
static DETACHED: AtomicBool = AtomicBool::new(false);
const CONTEXT: usize = 0x8756;
// Resident kernel fixture identity, not an installed R4D module or hardware.
const OWNER: u32 = 0x8756_0001;
unsafe extern "C" fn callback(irq: u8, context: usize) -> u32 {
    if irq != LINE.load(Ordering::Acquire) || context != CONTEXT {
        return 0;
    }
    let rejected = unsafe {
        crate::driver::api_entry::r4os_rust_driver_irq_register(irq, callback, CONTEXT, 4)
    } == -1;
    DETACHED.store(!runtime::held() && rejected, Ordering::Release);
    if HITS.fetch_add(1, Ordering::AcqRel) == 0 {
        ENTERED.store(true, Ordering::Release);
        // IF remains clear, so use the IRQ-independent physical clock and
        // service the independent TLB owner instead of a timer/yield loop.
        let start = monotonic::now_nanoseconds().expect("IRQ test requires monotonic clock");
        while !DONE.load(Ordering::Acquire) {
            if monotonic::now_nanoseconds()
                .unwrap_or(start)
                .saturating_sub(start)
                >= 250_000_000
            {
                break;
            }
            tlb::service_pending();
            core::hint::spin_loop();
        }
    }
    1
}
unsafe extern "C" fn observer() {
    let begin = timer::tick_count();
    while !ENTERED.load(Ordering::Acquire) {
        if timer::tick_count().saturating_sub(begin) > 1000 {
            DONE.store(true, Ordering::Release);
            return;
        }
        yield_task();
    }
    let irq = LINE.load(Ordering::Acquire);
    let result = unsafe { irq_router::r4os_rust_irq_router_unregister(irq, callback, CONTEXT) };
    // A retained retiring entry must reject resurrection of the same binding.
    let duplicate =
        unsafe { irq_router::r4os_rust_irq_router_register(irq, callback, CONTEXT, 4, OWNER) };
    let cleanup = irq_router::cleanup(OWNER);
    OBSERVED.store(
        if result == -3 && duplicate == -6 && !cleanup.quiesced && cleanup.removed == 0 {
            0
        } else {
            -1
        },
        Ordering::Release,
    );
    DONE.store(true, Ordering::Release);
}
pub(super) fn run() -> bool {
    if !kernel_check::live() {
        return true;
    }
    let Some(cpu) =
        (1..crate::arch::smp_policy::MAX_CPUS as u32).find(|&c| percpu::is_schedulable(c))
    else {
        puts(b"IRQ56 SKIP parallel-cpu-unavailable\r\n");
        return true;
    };
    // The explicit fixture never unmasks hardware: flag4 belongs to caller-
    // managed MSI delivery, and this resident callback is dispatched directly.
    let mut line = None;
    for irq in (24..32).rev() {
        let mut stats = crate::contract::IrqStats::default();
        unsafe {
            irq_router::r4os_rust_irq_router_stats(
                irq,
                false,
                (&mut stats as *mut crate::contract::IrqStats).cast(),
            );
        }
        if stats.registered == 0
            && unsafe {
                irq_router::r4os_rust_irq_router_register(irq, callback, CONTEXT, 4, OWNER)
            } == 0
        {
            line = Some(irq);
            break;
        }
    }
    let Some(irq) = line else {
        return fail(b"irq56-registration");
    };
    LINE.store(irq, Ordering::Release);
    ENTERED.store(false, Ordering::Release);
    DONE.store(false, Ordering::Release);
    OBSERVED.store(-99, Ordering::Release);
    HITS.store(0, Ordering::Release);
    DETACHED.store(false, Ordering::Release);
    let task =
        kernel_check::tasks::create_on_cpu(b"irq56-observer", observer, Role::Interactive, cpu);
    if task.is_null() {
        unsafe {
            irq_router::r4os_rust_irq_router_unregister(irq, callback, CONTEXT);
        }
        return fail(b"irq56-observer-create");
    }
    let started = kernel_check::tasks::start(task);
    if started {
        unsafe {
            irq_router::dispatch_observed(irq, 0);
        }
    }
    let first = started
        && DONE.load(Ordering::Acquire)
        && OBSERVED.load(Ordering::Acquire) == 0
        && DETACHED.load(Ordering::Acquire)
        && HITS.load(Ordering::Acquire) == 1;
    let quiesced = irq_router::retire_owner(OWNER);
    let provider_removed =
        unsafe { irq_router::r4os_rust_irq_router_unregister(irq, callback, CONTEXT) } == 0;
    let cleanup = irq_router::cleanup(OWNER);
    let removed = quiesced && provider_removed && cleanup.quiesced && cleanup.removed == 0;
    let mut reused = false;
    if first
        && removed
        && unsafe { irq_router::r4os_rust_irq_router_register(irq, callback, CONTEXT, 4, OWNER) }
            == 0
    {
        unsafe {
            irq_router::dispatch_observed(irq, 0);
        }
        reused = HITS.load(Ordering::Acquire) == 2 && DETACHED.load(Ordering::Acquire);
        unsafe { irq_router::withdraw_and_drain(irq, callback, CONTEXT) };
        let mut stats = crate::contract::IrqStats::default();
        unsafe {
            irq_router::r4os_rust_irq_router_stats(
                irq,
                false,
                (&mut stats as *mut crate::contract::IrqStats).cast(),
            );
        }
        reused &= stats.registered == 0;
    }
    let released = kernel_check::tasks::finish_task(task, true);
    if !(first && removed && reused && released) {
        return fail(b"irq56-progress-retire-reuse");
    }
    puts(b"IRQ56 PASS runtime-progress=parallel retire=busy context=retained removal=after-return reuse=new-generation observer=reaped software-vector=yes owner-drain=verified irq-register=denied provider-unregister=preserved\r\n");
    true
}
