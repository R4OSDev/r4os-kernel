//! Narrow checked instructions. CPU/register/value authority stays with the
//! calling hardware owner; this is not a public arbitrary register interface.
use super::{interrupt_frame::InterruptFrame, percpu, runtime, smp_policy::MAX_CPUS};
use core::{
    ptr,
    sync::atomic::{AtomicU32, AtomicU64, Ordering},
};

#[cfg(target_os = "none")]
core::arch::global_asm!(include_str!("safe_msr.S"), options(att_syntax));

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Error {
    Busy,
    Fault,
}
const PREPARING: u32 = 1;
const READ: u32 = 2;
const WRITE: u32 = 3;
struct Slot {
    active: AtomicU32,
    register: AtomicU32,
    epoch: AtomicU64,
    faults: AtomicU64,
}
impl Slot {
    const fn new() -> Self {
        Self {
            active: AtomicU32::new(0),
            register: AtomicU32::new(0),
            epoch: AtomicU64::new(0),
            faults: AtomicU64::new(0),
        }
    }
}
static SLOTS: [Slot; MAX_CPUS] = [const { Slot::new() }; MAX_CPUS];
unsafe extern "C" {
    fn r4os_safe_msr_read(id: u32, out: *mut u64) -> i32;
    fn r4os_safe_msr_write(id: u32, value: u64) -> i32;
    static r4os_safe_msr_read_fault: u8;
    static r4os_safe_msr_read_resume: u8;
    static r4os_safe_msr_write_fault: u8;
    static r4os_safe_msr_write_resume: u8;
}
struct Guard {
    cpu: usize,
    flags: u64,
}
impl Guard {
    fn acquire(id: u32, operation: u32) -> Result<Self, Error> {
        Self::acquire_exact(id, operation, None)
    }
    fn acquire_exact(id: u32, operation: u32, expected: Option<(u32, u64)>) -> Result<Self, Error> {
        let flags = runtime::save_local();
        let cpu = percpu::current_index() as usize;
        let epoch = crate::platform::cpu_topology::epoch(cpu as u32);
        if expected.is_some_and(|identity| identity != (cpu as u32, epoch) || epoch == 0) {
            runtime::restore_local(flags);
            return Err(Error::Busy);
        }
        let slot = &SLOTS[cpu];
        if slot
            .active
            .compare_exchange(0, PREPARING, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            runtime::restore_local(flags);
            return Err(Error::Busy);
        }
        slot.register.store(id, Ordering::Relaxed);
        slot.epoch.store(epoch, Ordering::Relaxed);
        slot.active.store(operation, Ordering::Release);
        Ok(Self { cpu, flags })
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        SLOTS[self.cpu].active.store(0, Ordering::Release);
        runtime::restore_local(self.flags);
    }
}
/// # Safety
/// The exact CPU has loaded the R4OS IDT. The caller has established feature,
/// vendor/model and register ownership; no arbitrary scanning is authorized.
/// The IF=0 call cannot sleep, migrate or invoke foreign code. Epoch0 is only
/// the sole early BSP before topology publication, never a public request.
pub(crate) unsafe fn read(id: u32) -> Result<u64, Error> {
    let _guard = Guard::acquire(id, READ)?;
    let mut value = 0;
    if unsafe { r4os_safe_msr_read(id, ptr::addr_of_mut!(value)) } == 0 {
        Ok(value)
    } else {
        Err(Error::Fault)
    }
}
/// # Safety
/// Same local IDT/feature/epoch contract as read, plus exact exclusive register
/// ownership and a model-valid value with every reserved bit preserved.
pub(crate) unsafe fn write(id: u32, value: u64) -> Result<(), Error> {
    let _guard = Guard::acquire(id, WRITE)?;
    if unsafe { r4os_safe_msr_write(id, value) } == 0 {
        Ok(())
    } else {
        Err(Error::Fault)
    }
}
/// # Safety
/// Same authority and local IDT contract as read. Expected CPU/epoch is
/// rechecked after CLI, including a preemption immediately before this call.
pub(crate) unsafe fn read_exact(cpu: u32, epoch: u64, id: u32) -> Result<u64, Error> {
    let _guard = Guard::acquire_exact(id, READ, Some((cpu, epoch)))?;
    let mut value = 0;
    if unsafe { r4os_safe_msr_read(id, ptr::addr_of_mut!(value)) } == 0 {
        Ok(value)
    } else {
        Err(Error::Fault)
    }
}
/// # Safety
/// Same authority and model-valid value contract as write, with exact local
/// CPU/epoch recheck. Only the explicit reserved-ID diagnostic uses this.
pub(crate) unsafe fn write_exact(cpu: u32, epoch: u64, id: u32, value: u64) -> Result<(), Error> {
    let _guard = Guard::acquire_exact(id, WRITE, Some((cpu, epoch)))?;
    if unsafe { r4os_safe_msr_write(id, value) } == 0 {
        Ok(())
    } else {
        Err(Error::Fault)
    }
}
pub(crate) fn recovered_faults(index: u32) -> u64 {
    SLOTS
        .get(index as usize)
        .map_or(0, |slot| slot.faults.load(Ordering::Acquire))
}
/// # Safety
/// The original ISR owns this complete exclusive resident frame. This path
/// performs no allocation, owner acquisition, FPU operation, wait or callback.
pub(crate) unsafe fn recover(frame: *mut InterruptFrame) -> bool {
    if frame.is_null() || !frame.is_aligned() {
        return false;
    }
    // Do not create a shared Rust frame reference across the exact RIP write.
    unsafe {
        if (*frame).vector != 13
            || (*frame).error_code != 0
            || (*frame).cs != 8
            || (*frame).rflags & (1 << 9) != 0
        {
            return false;
        }
        let cpu = percpu::current_index();
        let Some(slot) = SLOTS.get(cpu as usize) else {
            return false;
        };
        let operation = slot.active.load(Ordering::Acquire);
        let (fault, resume) = match operation {
            READ => (
                ptr::addr_of!(r4os_safe_msr_read_fault),
                ptr::addr_of!(r4os_safe_msr_read_resume),
            ),
            WRITE => (
                ptr::addr_of!(r4os_safe_msr_write_fault),
                ptr::addr_of!(r4os_safe_msr_write_resume),
            ),
            _ => return false,
        };
        if (*frame).rip != fault as u64
            || (*frame).rcx != u64::from(slot.register.load(Ordering::Relaxed))
            || crate::platform::cpu_topology::epoch(cpu) != slot.epoch.load(Ordering::Relaxed)
        {
            return false;
        }
        // This exact CPU is the sole writer with IF=0. Saturation preserves
        // the operational counter contract instead of fabricating a wrap.
        let old = slot.faults.load(Ordering::Relaxed);
        slot.faults.store(old.saturating_add(1), Ordering::Relaxed);
        ptr::addr_of_mut!((*frame).rip).write(resume as u64);
        true
    }
}
