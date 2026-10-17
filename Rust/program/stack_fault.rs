//! Latest resident stack failure per CPU. Recording never allocates, locks,
//! waits, scans a registry or reads the failing stack. Identities come only
//! from the current task's retained execution binding. No high-water scan.
use super::{async_io, stack, threads};
use crate::{
    arch::{percpu, smp_policy::MAX_CPUS, tables},
    contract::{self as c, ProgramStackFaultInfo as Info},
    memory::owner,
    sched::{
        scheduler,
        task::{task_registry, task_resources},
    },
};
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering, fence};

#[derive(Clone, Copy)]
pub(super) struct Failure {
    pub reason: u32,
    pub vm_error: u32,
}
impl Failure {
    pub const fn new(reason: u32) -> Self {
        Self {
            reason,
            vm_error: 0,
        }
    }
    pub const fn vm(reason: u32, code: u32) -> Self {
        Self {
            reason,
            vm_error: code,
        }
    }
}

// Atomic fields avoid the data race of an UnsafeCell-based seqlock. Writers
// own the local CPU with IF off; nested writers decline rather than waiting.
macro_rules! snapshot_fields {
    ($($field:ident: $atomic:ident),* $(,)?) => {
        struct Slot {
            revision: AtomicU64,
            prepared_failure: AtomicBool,
            tag_low: AtomicU64,
            tag_high: AtomicU64,
            $($field: $atomic,)*
        }
        impl Slot {
            const fn new() -> Self { Self {
                revision: AtomicU64::new(0),
                prepared_failure: AtomicBool::new(false),
                tag_low: AtomicU64::new(0),
                tag_high: AtomicU64::new(0),
                $($field: $atomic::new(0),)*
            } }
            fn publish(&self, info: &Info) {
                let revision = self.revision.load(Ordering::Relaxed);
                if revision & 1 != 0 || revision > u64::MAX - 2
                    || self.revision.compare_exchange(revision, revision + 1,
                        Ordering::Acquire, Ordering::Relaxed).is_err() {
                    return;
                }
                $(self.$field.store(info.$field, Ordering::Relaxed);)*
                self.tag_low.store(u64::from_ne_bytes(info.memory_tag[..8].try_into().unwrap()), Ordering::Relaxed);
                self.tag_high.store(u64::from_ne_bytes(info.memory_tag[8..].try_into().unwrap()), Ordering::Relaxed);
                self.revision.store(revision + 2, Ordering::Release);
            }
            fn read(&self, cpu: u32) -> Option<Info> {
                for _ in 0..3 {
                    let before = self.revision.load(Ordering::Acquire);
                    if before & 1 != 0 { continue; }
                    let mut info = Info {
                        cpu_index: cpu, sequence: before,
                        $($field: self.$field.load(Ordering::Relaxed),)*
                        ..Info::default()
                    };
                    info.memory_tag[..8].copy_from_slice(&self.tag_low.load(Ordering::Relaxed).to_ne_bytes());
                    info.memory_tag[8..].copy_from_slice(&self.tag_high.load(Ordering::Relaxed).to_ne_bytes());
                    fence(Ordering::Acquire);
                    if self.revision.load(Ordering::Relaxed) == before { return Some(info); }
                }
                None
            }
        }
    };
}
snapshot_fields! {
    flags: AtomicU32, stack_kind: AtomicU32, reason: AtomicU32,
    vm_error: AtomicU32, exception_vector: AtomicU32,
    task_id: AtomicU32, instance_id: AtomicU32, thread_id: AtomicU32,
    task_generation: AtomicU64, instance_generation: AtomicU64, thread_generation: AtomicU64,
    fault_address: AtomicU64, interrupted_rsp: AtomicU64, page_fault_error: AtomicU64,
    reserve_base: AtomicU64, reserve_bytes: AtomicU64, committed_base: AtomicU64,
    committed_bytes: AtomicU64, guard_base: AtomicU64, guard_bytes: AtomicU64,
    stack_top: AtomicU64, image_base: AtomicU64, image_bytes: AtomicU64,
}
static SLOTS: [Slot; MAX_CPUS] = [const { Slot::new() }; MAX_CPUS];

fn local() -> &'static Slot {
    &SLOTS[percpu::current_index() as usize]
}

/// Called once at the original page-fault entry, before any preparation.
pub(crate) fn begin_fault() {
    local().prepared_failure.store(false, Ordering::Relaxed);
}

/// A failed resident continuation is terminal when the original fault cannot
/// be handled on the exception IST. Preserve the failing operation's reason.
pub(super) unsafe fn preparation_failed(address: u64, rsp: u64, failure: Failure) {
    unsafe { capture(14, address, rsp, 0, Some(failure)) };
    local().prepared_failure.store(true, Ordering::Relaxed);
}

pub(super) unsafe fn growth_failed(address: u64, rsp: u64, error: u64, failure: Failure) {
    unsafe { capture(14, address, rsp, error, Some(failure)) };
    local().prepared_failure.store(true, Ordering::Relaxed);
}

/// # Safety
/// Original exception frame and current task binding remain resident with IF
/// off. The failure reason must belong to this exact exception if supplied.
unsafe fn capture(vector: u32, address: u64, rsp: u64, error: u64, failure: Option<Failure>) {
    let mut info = Info {
        cpu_index: percpu::current_index(),
        exception_vector: vector,
        fault_address: address,
        interrupted_rsp: rsp,
        page_fault_error: error,
        ..Info::default()
    };
    unsafe {
        let current = scheduler::current();
        if !current.is_null() {
            let task = task_registry::core(current.cast());
            info.task_id = (*task).id;
            info.task_generation = (*task).generation;
            info.flags |= c::stack_fault_flag_task;
            let thread = threads::current();
            let instance = if !thread.is_null() {
                info.thread_id = (*thread).id;
                info.thread_generation = (*thread).generation;
                info.flags |= c::stack_fault_flag_thread;
                info.instance_id = (*thread).instance_id;
                info.instance_generation = (*thread).instance_generation;
                (*thread).owner_instance
            } else if let Some(binding) = async_io::execution() {
                info.instance_id = binding.id;
                info.instance_generation = binding.generation;
                binding.instance
            } else {
                core::ptr::null_mut()
            };
            if !instance.is_null()
                && (*instance).id == info.instance_id
                && info.instance_generation != 0
            {
                info.flags |= c::stack_fault_flag_instance;
                info.image_base = (*instance).program_image_base;
                info.image_bytes = (*instance).program_image_size as u64;
                info.memory_tag = (*instance).memory_tag;
            } else {
                info.instance_id = 0;
                info.instance_generation = 0;
            }
            if !thread.is_null() {
                let main = (*thread).flags & 1 != 0;
                let program = if main && info.flags & c::stack_fault_flag_instance != 0 {
                    stack::from_instance(instance)
                } else if !main {
                    Some((*thread).stack)
                } else {
                    None
                };
                if let Some(s) = program
                    && (within(address, s.base, s.reserve_size)
                        || within_rsp(rsp, s.base, s.reserve_size))
                {
                    info.stack_kind = if main {
                        c::stack_fault_kind_program_main
                    } else {
                        c::stack_fault_kind_program_thread
                    };
                    bounds(
                        &mut info,
                        s.base,
                        s.reserve_size,
                        s.committed_base,
                        s.committed_size,
                        s.guard_base,
                        s.guard_size,
                        s.top,
                    );
                }
            }
            if info.flags & c::stack_fault_flag_bounds == 0 && (*task).stack_range_id != 0 {
                let base = (*task).stack_base;
                let top = (*task).stack_top;
                let reserve = base.saturating_sub(task_resources::GUARD_SIZE);
                if top > base
                    && (within(address, reserve, top - reserve)
                        || within_rsp(rsp, base, top - base))
                {
                    info.stack_kind = c::stack_fault_kind_kernel_task;
                    bounds(
                        &mut info,
                        reserve,
                        top - reserve,
                        base,
                        top - base,
                        reserve,
                        task_resources::GUARD_SIZE,
                        top,
                    );
                }
            }
        }
        if info.flags & c::stack_fault_flag_bounds == 0
            && let Some((base, top)) = tables::exception_stack_bounds(rsp)
        {
            info.stack_kind = c::stack_fault_kind_exception;
            bounds(&mut info, base, top - base, base, top - base, 0, 0, top);
        }
    }
    let cause = failure.unwrap_or_else(|| classify(&info, owner::fault_allowed()));
    info.reason = cause.reason;
    info.vm_error = cause.vm_error;
    local().publish(&info);
}

fn within(address: u64, base: u64, len: u64) -> bool {
    address >= base && address - base < len
}
fn within_rsp(rsp: u64, base: u64, len: u64) -> bool {
    rsp.checked_sub(1)
        .is_some_and(|address| within(address, base, len))
}
#[allow(clippy::too_many_arguments)]
fn bounds(
    info: &mut Info,
    base: u64,
    reserve: u64,
    committed_base: u64,
    committed: u64,
    guard_base: u64,
    guard: u64,
    top: u64,
) {
    info.flags |= c::stack_fault_flag_bounds;
    info.reserve_base = base;
    info.reserve_bytes = reserve;
    info.committed_base = committed_base;
    info.committed_bytes = committed;
    info.guard_base = guard_base;
    info.guard_bytes = guard;
    info.stack_top = top;
}
fn classify(info: &Info, growth_allowed: bool) -> Failure {
    let address = info.fault_address;
    let reason = if info.exception_vector != 14 || info.flags & c::stack_fault_flag_bounds == 0 {
        c::stack_fault_reason_unknown
    } else if info.stack_kind == c::stack_fault_kind_kernel_task
        && within(address, info.guard_base, info.guard_bytes)
    {
        c::stack_fault_reason_kernel_guard
    } else if matches!(
        info.stack_kind,
        c::stack_fault_kind_program_main | c::stack_fault_kind_program_thread
    ) && within(address, info.reserve_base, info.reserve_bytes)
    {
        if address - info.reserve_base < info.guard_bytes {
            c::stack_fault_reason_reserve_exhausted
        } else if info.page_fault_error & 1 != 0 {
            c::stack_fault_reason_protection
        } else if address < info.committed_base && !growth_allowed {
            c::stack_fault_reason_growth_forbidden
        } else {
            c::stack_fault_reason_unknown
        }
    } else {
        c::stack_fault_reason_unknown
    };
    Failure::new(reason)
}

/// Publish any otherwise unhandled exception, preserving preparation/growth
/// details already captured for this entry. This does not handle the fault.
pub(crate) unsafe fn exception(vector: u32, address: u64, rsp: u64, error: u64) {
    if vector == 14
        && local().prepared_failure.swap(false, Ordering::Relaxed)
        && let Some(mut info) = local().read(percpu::current_index())
        && info.fault_address == address
        && info.interrupted_rsp == rsp
    {
        info.page_fault_error = error;
        local().publish(&info);
        return;
    }
    unsafe { capture(vector, address, rsp, error, None) };
}

/// # Safety
/// Caller retains a writable canonical header and advertised output capacity.
pub(crate) unsafe extern "C" fn snapshot(index: u32, out: *mut Info) -> i32 {
    unsafe {
        if (*out).version != 1 || (*out).size < core::mem::size_of::<Info>() as u32 {
            return -1;
        }
    }
    let Some(slot) = SLOTS.get(index as usize) else {
        return 0;
    };
    let Some(info) = slot.read(index) else {
        return -2;
    };
    unsafe { out.write(info) };
    1
}

/// Existing crash serial route, with only bounded scalar formatting frames.
pub(crate) fn emit() {
    use crate::boot::log_route as l;
    let Some(info) = local().read(percpu::current_index()) else {
        return;
    };
    l::puts(b"[STACKFAULT] kind=");
    l::puts(kind_name(info.stack_kind));
    l::puts(b" reason=");
    l::puts(reason_name(info.reason));
    l::puts(b" vm_error=");
    l::dec(u64::from(info.vm_error));
    l::puts(b" available=");
    l::hex(u64::from(info.flags), 8);
    l::puts(b" task=");
    l::dec(u64::from(info.task_id));
    l::puts(b"/");
    l::dec(info.task_generation);
    l::puts(b" instance=");
    l::dec(u64::from(info.instance_id));
    l::puts(b"/");
    l::dec(info.instance_generation);
    l::puts(b" thread=");
    l::dec(u64::from(info.thread_id));
    l::puts(b"/");
    l::dec(info.thread_generation);
    l::puts(b" tag=");
    l::puts(&info.memory_tag[..info.memory_tag.iter().position(|&b| b == 0).unwrap_or(16)]);
    l::puts(b" fault=0x");
    l::hex(info.fault_address, 16);
    l::puts(b" rsp=0x");
    l::hex(info.interrupted_rsp, 16);
    l::puts(b"\r\n[STACKFAULT] reserve=0x");
    l::hex(info.reserve_base, 16);
    l::puts(b" bytes=");
    l::dec(info.reserve_bytes);
    l::puts(b" committed=0x");
    l::hex(info.committed_base, 16);
    l::puts(b" bytes=");
    l::dec(info.committed_bytes);
    l::puts(b" guard=0x");
    l::hex(info.guard_base, 16);
    l::puts(b" bytes=");
    l::dec(info.guard_bytes);
    l::puts(b" top=0x");
    l::hex(info.stack_top, 16);
    l::puts(b"\r\n");
}

pub(crate) fn for_report(report: &crate::boot::crash::CrashReport) -> Option<Info> {
    let info = local().read(percpu::current_index())?;
    (info.sequence != 0
        && u64::from(info.exception_vector) == u64::from(report.cpu.vector)
        && info.interrupted_rsp == report.cpu.rsp
        && (info.exception_vector != 14 || info.fault_address == report.page_fault.fault_address))
        .then_some(info)
}

pub(crate) fn kind_name(kind: u32) -> &'static [u8] {
    match kind {
        c::stack_fault_kind_program_main => b"program-main",
        c::stack_fault_kind_program_thread => b"program-thread",
        c::stack_fault_kind_kernel_task => b"kernel-task",
        c::stack_fault_kind_exception => b"exception",
        _ => b"unavailable",
    }
}
pub(crate) fn reason_name(reason: u32) -> &'static [u8] {
    match reason {
        c::stack_fault_reason_reserve_exhausted => b"reserve-exhausted",
        c::stack_fault_reason_kernel_guard => b"kernel-guard",
        c::stack_fault_reason_physical_budget => b"physical-budget",
        c::stack_fault_reason_vm_commit => b"vm-commit",
        c::stack_fault_reason_growth_forbidden => b"growth-forbidden",
        c::stack_fault_reason_protection => b"protection",
        c::stack_fault_reason_vm_guard => b"vm-guard",
        c::stack_fault_reason_continuation_headroom => b"continuation-headroom",
        _ => b"unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_only_proven_stack_failures() {
        let mut info = Info {
            exception_vector: 14,
            stack_kind: c::stack_fault_kind_program_main,
            reserve_base: 0x10_0000,
            reserve_bytes: 0x10_0000,
            committed_base: 0x1f_0000,
            guard_bytes: 4096,
            fault_address: 0x10_0000,
            ..Info::default()
        };
        assert_eq!(classify(&info, false).reason, c::stack_fault_reason_unknown);
        info.flags = c::stack_fault_flag_bounds;
        assert_eq!(
            classify(&info, true).reason,
            c::stack_fault_reason_reserve_exhausted
        );
        info.fault_address = 0x1e_0000;
        assert_eq!(
            classify(&info, false).reason,
            c::stack_fault_reason_growth_forbidden
        );
        assert_eq!(classify(&info, true).reason, c::stack_fault_reason_unknown);
        info.page_fault_error = 3;
        assert_eq!(
            classify(&info, false).reason,
            c::stack_fault_reason_protection
        );
        info.stack_kind = c::stack_fault_kind_kernel_task;
        info.guard_base = info.reserve_base;
        info.fault_address = info.guard_base + 4095;
        assert_eq!(
            classify(&info, false).reason,
            c::stack_fault_reason_kernel_guard
        );
        info.fault_address += 1;
        assert_eq!(classify(&info, false).reason, c::stack_fault_reason_unknown);
        info.fault_address = info.guard_base;
        info.exception_vector = 8;
        assert_eq!(classify(&info, false).reason, c::stack_fault_reason_unknown);
    }

    #[test]
    fn snapshot_has_bounded_busy_and_keeps_exact_generations() {
        let slot = Slot::new();
        assert_eq!(slot.read(3).unwrap().sequence, 0);
        let info = Info {
            task_generation: u64::MAX - 3,
            instance_generation: 1 << 62,
            thread_generation: (1 << 61) + 7,
            flags: c::stack_fault_flag_task
                | c::stack_fault_flag_instance
                | c::stack_fault_flag_thread,
            vm_error: 12,
            reason: c::stack_fault_reason_physical_budget,
            memory_tag: *b"0123456789abcdef",
            ..Info::default()
        };
        slot.publish(&info);
        let got = slot.read(3).unwrap();
        assert_eq!(got.cpu_index, 3);
        assert_eq!(got.sequence, 2);
        assert_eq!(got.task_generation, info.task_generation);
        assert_eq!(got.instance_generation, info.instance_generation);
        assert_eq!(got.thread_generation, info.thread_generation);
        assert_eq!(got.memory_tag, info.memory_tag);
        assert_eq!(got.vm_error, 12);
        assert_eq!(got.reason, c::stack_fault_reason_physical_budget);
        slot.revision.store(3, Ordering::Relaxed);
        assert!(slot.read(3).is_none());
        slot.publish(&Info::default()); // nested writer must decline, not wait
        assert_eq!(slot.revision.load(Ordering::Relaxed), 3);
        slot.revision.store(u64::MAX - 1, Ordering::Relaxed);
        slot.publish(&Info::default()); // revision exhaustion must not wrap
        assert_eq!(slot.revision.load(Ordering::Relaxed), u64::MAX - 1);
    }

    #[test]
    fn concurrent_reader_never_accepts_mixed_failure_identity() {
        let slot = std::sync::Arc::new(Slot::new());
        let writer_slot = slot.clone();
        let writer = std::thread::spawn(move || {
            for generation in 1u64..=2000 {
                writer_slot.publish(&Info {
                    task_generation: generation,
                    instance_generation: generation ^ u64::MAX,
                    thread_generation: generation * 3,
                    memory_tag: generation.to_ne_bytes().repeat(2).try_into().unwrap(),
                    ..Info::default()
                });
            }
        });
        while !writer.is_finished() {
            if let Some(info) = slot.read(0)
                && info.sequence != 0
            {
                assert_eq!(info.instance_generation, info.task_generation ^ u64::MAX);
                assert_eq!(info.thread_generation, info.task_generation * 3);
                assert_eq!(info.memory_tag[..8], info.task_generation.to_ne_bytes());
                assert_eq!(info.memory_tag[8..], info.task_generation.to_ne_bytes());
            }
        }
        writer.join().unwrap();
        assert_eq!(slot.read(0).unwrap().task_generation, 2000);
    }
}
