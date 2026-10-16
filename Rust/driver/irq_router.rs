//! Original 32-vector/four-handler route owner. Runtime admission serializes
//! tables; per-CPU dispatch identity stays local. No Rust reference spans R4D.
use crate::{
    arch::{ioapic, percpu, pic, runtime, smp_policy::MAX_CPUS},
    build_config::config,
    contract,
    platform::{monotonic, timer},
    sched::scheduler,
};
use core::{
    cell::UnsafeCell,
    mem::{self, MaybeUninit},
    ptr,
};
pub(crate) const MAX_IRQS: usize = 32;
const MAX_HANDLERS: usize = 4;
pub type Handler = unsafe extern "C" fn(u8, usize) -> u32;
#[derive(Clone, Copy)]
struct Entry {
    active: bool,
    shared: bool,
    handler: Option<Handler>,
    context: usize,
    owner: u32,
}
impl Entry {
    const EMPTY: Self = Self {
        active: false,
        shared: false,
        handler: None,
        context: 0,
        owner: 0,
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Timing {
    pub dispatch_samples: u64,
    pub handler_samples: u64,
    pub observer_reads: u64,
    pub dispatch_total_ns: u64,
    pub dispatch_max_ns: u64,
    pub dispatch_last_ns: u64,
    pub handler_total_ns: u64,
    pub handler_max_ns: u64,
    pub handler_last_ns: u64,
    pub clock_generation: u32,
    pub unavailable_samples: u32,
    pub mixed_generation: bool,
}
impl Timing {
    const EMPTY: Self = Self {
        dispatch_samples: 0,
        handler_samples: 0,
        observer_reads: 0,
        dispatch_total_ns: 0,
        dispatch_max_ns: 0,
        dispatch_last_ns: 0,
        handler_total_ns: 0,
        handler_max_ns: 0,
        handler_last_ns: 0,
        clock_generation: 0,
        unavailable_samples: 0,
        mixed_generation: false,
    };
}
const EMPTY_STATS: contract::IrqStats = contract::IrqStats {
    irq: 0,
    registered: 0,
    shared: 0,
    masked: 1,
    dispatch_count: 0,
    handled_count: 0,
    last_result: 0,
    reserved: 0,
    handler_total_ticks: 0,
    handler_max_ticks: 0,
    handler_last_ticks: 0,
    last_owner: 0,
};
struct State {
    entries: [[Entry; MAX_HANDLERS]; MAX_IRQS],
    stats: [contract::IrqStats; MAX_IRQS],
    timing: [Timing; MAX_IRQS],
    depth: [u32; MAX_CPUS],
    owner: [u32; MAX_CPUS],
    bad_reported: bool,
    bad_count: u64,
}
const fn kernel_owned(irq: u8) -> bool {
    irq == 0 || irq == 1 || irq == 12
}
const fn initial() -> State {
    let mut s = State {
        entries: [[Entry::EMPTY; MAX_HANDLERS]; MAX_IRQS],
        stats: [EMPTY_STATS; MAX_IRQS],
        timing: [Timing::EMPTY; MAX_IRQS],
        depth: [0; MAX_CPUS],
        owner: [0; MAX_CPUS],
        bad_reported: false,
        bad_count: 0,
    };
    let mut i = 0;
    while i < MAX_IRQS {
        s.stats[i].irq = i as u8;
        s.stats[i].masked = if kernel_owned(i as u8) { 0 } else { 1 };
        i += 1;
    }
    s
}
struct Shared(UnsafeCell<State>);
unsafe impl Sync for Shared {}
static STATE: Shared = Shared(UnsafeCell::new(initial()));

fn raw() -> *mut State {
    STATE.0.get()
}
fn plausible(handler: Handler) -> bool {
    handler as usize as u64 >= 0xffff_8000_0000_0000
}
fn report(where_code: u8, irq: u8, handler: Handler, owner: u32) {
    let flags = runtime::save_runtime();
    let publish = unsafe {
        let s = raw();
        (*s).bad_count += 1;
        let first = !(*s).bad_reported;
        (*s).bad_reported = true;
        first
    };
    runtime::restore_runtime(flags);
    if publish {
        unsafe { irq_router_bad_handler(where_code, irq, handler as usize as u64, owner) };
    }
}
unsafe fn entry(index: usize, slot: usize) -> *mut Entry {
    unsafe {
        ptr::addr_of_mut!((*raw()).entries)
            .cast::<Entry>()
            .add(index * MAX_HANDLERS + slot)
    }
}
unsafe fn stats(index: usize) -> *mut contract::IrqStats {
    unsafe {
        ptr::addr_of_mut!((*raw()).stats)
            .cast::<contract::IrqStats>()
            .add(index)
    }
}
unsafe fn timing(index: usize) -> *mut Timing {
    unsafe {
        ptr::addr_of_mut!((*raw()).timing)
            .cast::<Timing>()
            .add(index)
    }
}
fn handlers(index: usize) -> u8 {
    let mut count = 0;
    for slot in 0..MAX_HANDLERS {
        if unsafe { (*entry(index, slot)).active } {
            count += 1;
        }
    }
    count
}
fn refresh(index: usize) {
    let mut registered = 0;
    let mut shared = 0;
    for slot in 0..MAX_HANDLERS {
        let e = unsafe { entry(index, slot).read() };
        if e.active {
            registered += 1;
            if e.shared {
                shared += 1;
            }
        }
    }
    unsafe {
        (*stats(index)).irq = index as u8;
        (*stats(index)).registered = registered;
        (*stats(index)).shared = shared;
    }
}
fn enable(irq: u8, flags: u32) {
    let index = irq as usize;
    if flags & 4 != 0 {
        unsafe { (*stats(index)).masked = 0 };
        return;
    }
    if ioapic::r4os_rust_ioapic_control(0, 0, false) {
        if flags & 2 != 0 && ioapic::r4os_rust_ioapic_control(3, irq, false) {
            if irq < 16 {
                pic::unmask(irq);
            }
            unsafe { (*stats(index)).masked = 0 };
            return;
        }
        if ioapic::r4os_rust_ioapic_control(2, irq, false) {
            if irq < 16 {
                pic::unmask(irq);
            }
            unsafe { (*stats(index)).masked = 0 };
            return;
        }
    }
    if irq < 16 {
        pic::unmask(irq);
        unsafe { (*stats(index)).masked = 0 };
    }
}
fn disable(irq: u8) {
    if ioapic::r4os_rust_ioapic_control(0, 0, false) {
        ioapic::r4os_rust_ioapic_control(1, irq, true);
    } else if irq < 16 {
        pic::mask(irq);
    }
    unsafe { (*stats(irq as usize)).masked = 1 };
}
fn register_inner(irq: u8, handler: Handler, context: usize, flags: u32, owner: u32) -> i32 {
    let shared = flags & 1 != 0;
    let index = irq as usize;
    let mut free = None;
    let mut active = 0;
    let mut all_shared = true;
    for slot in 0..MAX_HANDLERS {
        let e = unsafe { entry(index, slot).read() };
        if !e.active {
            if free.is_none() {
                free = Some(slot);
            }
            continue;
        }
        if e.handler.is_some_and(|h| h as usize == handler as usize) && e.context == context {
            return 0;
        }
        active += 1;
        all_shared &= e.shared;
    }
    if active != 0 && (!shared || !all_shared) {
        return -2;
    }
    let Some(slot) = free else {
        return -3;
    };
    unsafe {
        entry(index, slot).write(Entry {
            active: true,
            shared,
            handler: Some(handler),
            context,
            owner,
        })
    };
    refresh(index);
    enable(irq, flags);
    0
}
/// # Safety
/// Handler is the exact executable R4D/builtin callback retained by its caller.
/// Registration preserves shared-line and explicit owner cleanup semantics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_irq_router_register(
    irq: u8,
    handler: Handler,
    context: usize,
    flags: u32,
    owner: u32,
) -> i32 {
    if irq as usize >= MAX_IRQS {
        return -1;
    }
    if !plausible(handler) {
        report(0, irq, handler, owner);
        return -4;
    }
    let saved = runtime::save_runtime();
    let result = register_inner(irq, handler, context, flags, owner);
    runtime::restore_runtime(saved);
    result
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_irq_router_cleanup(owner: u32) -> u32 {
    if owner == 0 {
        return 0;
    }
    let saved = runtime::save_runtime();
    let mut removed = 0;
    for index in 0..MAX_IRQS {
        let mut changed = false;
        for slot in 0..MAX_HANDLERS {
            let e = unsafe { entry(index, slot).read() };
            if !e.active || e.owner != owner {
                continue;
            }
            unsafe { entry(index, slot).write(Entry::EMPTY) };
            removed += 1;
            changed = true;
        }
        if changed {
            refresh(index);
            if handlers(index) == 0 && !kernel_owned(index as u8) {
                disable(index as u8);
            }
        }
    }
    runtime::restore_runtime(saved);
    removed
}
/// # Safety
/// Exact callback/context identity belongs to the live registering caller.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_irq_router_unregister(
    irq: u8,
    handler: Handler,
    context: usize,
) -> i32 {
    if irq as usize >= MAX_IRQS {
        return -1;
    }
    let saved = runtime::save_runtime();
    let index = irq as usize;
    let mut result = -2;
    for slot in 0..MAX_HANDLERS {
        let e = unsafe { entry(index, slot).read() };
        if !e.active
            || !e.handler.is_some_and(|h| h as usize == handler as usize)
            || e.context != context
        {
            continue;
        }
        unsafe { entry(index, slot).write(Entry::EMPTY) };
        refresh(index);
        if handlers(index) == 0 && !kernel_owned(irq) {
            disable(irq);
        }
        result = 0;
        break;
    }
    runtime::restore_runtime(saved);
    result
}
/// # Safety
/// Exclusive aligned output of the exact selected fixed C stats record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_irq_router_stats(
    irq: u8,
    detail: bool,
    out: *mut core::ffi::c_void,
) -> i32 {
    if irq as usize >= MAX_IRQS {
        return -1;
    }
    let saved = runtime::save_runtime();
    unsafe {
        if detail {
            out.cast::<Timing>().write(timing(irq as usize).read());
        } else {
            out.cast::<contract::IrqStats>()
                .write(stats(irq as usize).read());
        }
    }
    runtime::restore_runtime(saved);
    0
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_irq_router_local(owner: bool) -> u32 {
    let cpu = percpu::current_index() as usize;
    unsafe {
        if owner {
            (&(*raw()).owner)[cpu]
        } else {
            (&(*raw()).depth)[cpu]
        }
    }
}
fn measured(index: usize, start: monotonic::Stamp, end: monotonic::Stamp) -> Option<u64> {
    let t = unsafe { &mut *timing(index) };
    if start.generation != end.generation {
        t.mixed_generation = true;
        t.unavailable_samples = t.unavailable_samples.saturating_add(1);
        return None;
    }
    if t.clock_generation == 0 {
        t.clock_generation = start.generation;
    } else if t.clock_generation != start.generation {
        t.mixed_generation = true;
    }
    let elapsed = monotonic::elapsed(start, end);
    if elapsed.is_none() {
        t.unavailable_samples = t.unavailable_samples.saturating_add(1);
    }
    elapsed
}
fn record(index: usize, start: monotonic::Stamp, end: monotonic::Stamp, handler: bool) {
    let Some(elapsed) = measured(index, start, end) else {
        return;
    };
    let t = unsafe { &mut *timing(index) };
    if handler {
        t.handler_samples = t.handler_samples.saturating_add(1);
        t.handler_total_ns = t.handler_total_ns.saturating_add(elapsed);
        t.handler_last_ns = elapsed;
        t.handler_max_ns = t.handler_max_ns.max(elapsed);
    } else {
        t.dispatch_samples = t.dispatch_samples.saturating_add(1);
        t.dispatch_total_ns = t.dispatch_total_ns.saturating_add(elapsed);
        t.dispatch_last_ns = elapsed;
        t.dispatch_max_ns = t.dispatch_max_ns.max(elapsed);
    }
}
/// # Safety
/// Called within the existing IRQ runtime admission. Handlers retain their
/// module lifetime, cannot dispatch tasks, and use the exact native FPU guard.
/// No Rust state borrow or resource-owning Drop spans a foreign callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_irq_router_dispatch(irq: u8) {
    if irq as usize >= MAX_IRQS {
        return;
    }
    let index = irq as usize;
    let mut invoked = false;
    let mut last_result = 0;
    let measure_dispatch = config::ENABLE_METRICS && unsafe { (*stats(index)).registered != 0 };
    let dispatch_start = if measure_dispatch {
        monotonic::capture()
    } else {
        monotonic::Stamp::default()
    };
    if measure_dispatch {
        unsafe {
            (*timing(index)).observer_reads = (*timing(index)).observer_reads.saturating_add(1)
        };
    }
    let cpu = percpu::current_index() as usize;
    unsafe { (&mut (*raw()).depth)[cpu] = (&(*raw()).depth)[cpu].saturating_add(1) };
    for slot in 0..MAX_HANDLERS {
        let e = unsafe { entry(index, slot).read() };
        if !e.active {
            continue;
        }
        let Some(handler) = e.handler else {
            continue;
        };
        if !plausible(handler) {
            report(1, irq, handler, e.owner);
            continue;
        }
        invoked = true;
        unsafe { (&mut (*raw()).owner)[cpu] = e.owner };
        let start = if config::ENABLE_METRICS {
            timer::tick_count()
        } else {
            0
        };
        let timing_start = if config::ENABLE_METRICS {
            monotonic::capture()
        } else {
            monotonic::Stamp::default()
        };
        if config::ENABLE_METRICS {
            unsafe {
                (*timing(index)).observer_reads = (*timing(index)).observer_reads.saturating_add(1)
            };
        }
        let mut guard = MaybeUninit::<scheduler::ExternalIrqFpuGuard>::uninit();
        unsafe { scheduler::r4os_rust_scheduler_irq_enter(guard.as_mut_ptr()) };
        last_result = unsafe { handler(irq, e.context) };
        unsafe { scheduler::r4os_rust_scheduler_irq_leave(guard.as_ptr()) };
        if config::ENABLE_METRICS {
            let timing_end = monotonic::capture();
            unsafe {
                (*timing(index)).observer_reads = (*timing(index)).observer_reads.saturating_add(1)
            };
            let elapsed = timer::tick_count().saturating_sub(start);
            unsafe {
                let s = &mut *stats(index);
                s.handler_total_ticks = s.handler_total_ticks.wrapping_add(elapsed);
                s.handler_last_ticks = elapsed;
                s.handler_max_ticks = s.handler_max_ticks.max(elapsed);
            }
            record(index, timing_start, timing_end, true);
        }
        unsafe {
            (*stats(index)).last_owner = e.owner;
            (&mut (*raw()).owner)[cpu] = 0;
            if last_result & 1 != 0 {
                (*stats(index)).handled_count = (*stats(index)).handled_count.wrapping_add(1);
            }
        }
    }
    unsafe {
        if invoked {
            (*stats(index)).dispatch_count = (*stats(index)).dispatch_count.wrapping_add(1);
            (*stats(index)).last_result = last_result;
        }
    }
    if measure_dispatch {
        let dispatch_end = monotonic::capture();
        unsafe {
            (*timing(index)).observer_reads = (*timing(index)).observer_reads.saturating_add(1)
        };
        record(index, dispatch_start, dispatch_end, false);
    }
    unsafe { (&mut (*raw()).depth)[cpu] -= 1 };
}
const _: () = {
    assert!(mem::size_of::<Timing>() == 88);
    assert!(mem::size_of::<contract::IrqStats>() == 64);
};

// Typed passive projection of the existing CPU-local dispatch depth.
pub(crate) fn in_dispatch() -> bool {
    r4os_rust_irq_router_local(false) != 0
}

unsafe fn irq_router_bad_handler(where_code: u8, irq: u8, address: u64, owner: u32) {
    use crate::boot::log_route as l;
    l::puts(b"IRQROUTER BAD HANDLER ");
    l::puts(match where_code {
        0 => b"register",
        1 => b"dispatch",
        _ => panic!("Invalid IRQ handler diagnostic"),
    });
    l::puts(b" irq=");
    l::dec(u64::from(irq));
    l::puts(b" ptr=0x");
    l::hex(address, 16);
    l::puts(b" owner=");
    l::dec(u64::from(owner));
    l::puts(b"\r\n");
}
