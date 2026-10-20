//! Optional save strategy seals before RELEASE_EPOCH. Format/backend remain
//! standard XSAVE2. Only exact native task saves use the optional instruction.
use super::{fpu, percpu, smp_policy::MAX_CPUS};
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicU8, AtomicU64, Ordering},
};
pub(crate) mod policy {
    include!("xsaveopt_policy.rs");
}
use policy::Tag;
static MODE: AtomicU8 = AtomicU8::new(0); //0 pending/classic,1 sealedclassic,2 sealedOPT
static KNOWN: AtomicU64 = AtomicU64::new(0);
static SUPPORTED: AtomicU64 = AtomicU64::new(0);
#[derive(Clone, Copy, Default)]
struct Local {
    tag: Tag,
    classic: u64,
    opt: u64,
}
struct Locals(UnsafeCell<[Local; MAX_CPUS]>);
// Existing native task/runtime and local IF=0 admission own each CPU slot.
// No reference is retained across save/restore/foreign calls. Readout acquires
// runtime admission and happens outside any measured public workload.
unsafe impl Sync for Locals {}
static LOCALS: Locals = Locals(UnsafeCell::new(
    [Local {
        tag: Tag {
            id: 0,
            generation: 0,
            pointer: 0,
            valid: false,
        },
        classic: 0,
        opt: 0,
    }; MAX_CPUS],
));
pub(crate) fn local_supported() -> bool {
    let max = core::arch::x86_64::__cpuid_count(0, 0).eax;
    max >= 0x0d && core::arch::x86_64::__cpuid_count(0x0d, 1).eax & 1 != 0
}
pub(crate) fn report(index: u32, supported: bool) {
    if index >= MAX_CPUS as u32 {
        return;
    }
    let bit = 1u64 << index;
    if supported {
        SUPPORTED.fetch_or(bit, Ordering::Release);
    } else {
        SUPPORTED.fetch_and(!bit, Ordering::Release);
    }
    KNOWN.fetch_or(bit, Ordering::Release);
}
pub(crate) fn required() -> bool {
    MODE.load(Ordering::Acquire) == 2
}
pub(crate) fn seal(count: u32) {
    let d = fpu::descriptor();
    let use_opt = policy::eligible(
        count,
        KNOWN.load(Ordering::Acquire),
        SUPPORTED.load(Ordering::Acquire),
        crate::build_config::config::XSAVEOPT_POLICY == 1,
        d.is_some_and(|d| d.contract.standard()),
    );
    let mode = if use_opt { 2 } else { 1 };
    let _ = MODE.compare_exchange(0, mode, Ordering::Release, Ordering::Relaxed);
    let (flags, known, supported) = status();
    crate::boot::log_route::puts(b"[XSAVEOPT48] sealed strategy=");
    crate::boot::log_route::puts(if flags == 2 { b"xsaveopt" } else { b"classic" });
    for (label, value) in [
        (b" configured=".as_slice(), count as u64),
        (b" expected=", policy::expected(count)),
        (b" known=", known),
        (b" supported=", supported),
    ] {
        crate::boot::log_route::puts(label);
        crate::boot::log_route::dec(value);
    }
    crate::boot::log_route::puts(b" layout=standard decision_before_ap_release=1\r\n");
}
pub(crate) fn status() -> (u8, u64, u64) {
    (
        MODE.load(Ordering::Acquire),
        KNOWN.load(Ordering::Acquire),
        SUPPORTED.load(Ordering::Acquire),
    )
}
fn local() -> *mut Local {
    unsafe {
        LOCALS
            .0
            .get()
            .cast::<Local>()
            .add(percpu::current_index() as usize)
    }
}
pub(crate) fn clear() {
    unsafe {
        (*local()).tag.valid = false;
    }
}
pub(crate) fn reset(index: u32) {
    if index < MAX_CPUS as u32 {
        unsafe {
            (*LOCALS.0.get().cast::<Local>().add(index as usize))
                .tag
                .valid = false;
        }
    }
}
pub(crate) fn matching(id: u32, generation: u64, pointer: usize) -> bool {
    required() && unsafe { (*local()).tag.matches(id, generation, pointer) }
}
pub(crate) fn restored(id: u32, generation: u64, pointer: usize) {
    unsafe {
        (*local()).tag = Tag {
            id,
            generation,
            pointer,
            valid: true,
        };
    }
}
pub(crate) fn saved(opt: bool) {
    unsafe {
        let p = local();
        if opt {
            (*p).opt = (*p).opt.wrapping_add(1);
        } else {
            (*p).classic = (*p).classic.wrapping_add(1);
        }
    }
}
pub(crate) fn totals() -> (u64, u64) {
    let _r = super::runtime::RuntimeGuard::acquire();
    let mut c = 0u64;
    let mut o = 0u64;
    for i in 0..MAX_CPUS {
        // Generic local restore may invalidate only the tag without runtime;
        // copy only the counters actually serialized by the runtime owner.
        let p = unsafe { LOCALS.0.get().cast::<Local>().add(i) };
        c = c.wrapping_add(unsafe { core::ptr::addr_of!((*p).classic).read() });
        o = o.wrapping_add(unsafe { core::ptr::addr_of!((*p).opt).read() });
    }
    (c, o)
}
