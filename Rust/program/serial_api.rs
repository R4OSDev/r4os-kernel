//! Public COM entrypoints keep exact process/task pins, never a registry lock
//! across provider calls or waits. Caller bytes remain with their R4X owner.
use super::{context, registry, registry_types::ProgramInstanceLease};
use crate::{
    arch::runtime,
    contract as a,
    sched::{scheduler, task::task_registry, task_context},
    serial,
};
use core::{mem::MaybeUninit, ptr, sync::atomic::AtomicBool};
struct Caller {
    lease: ProgramInstanceLease,
    _native: task_context::Admission,
}
impl Caller {
    fn enter() -> Result<Self, i32> {
        unsafe {
            let flags: u64;
            core::arch::asm!("pushfq","pop {}",out(reg)flags,options(preserves_flags));
            if flags & (1 << 9) == 0
                || runtime::r4os_rust_runtime_control(8, 0) != 0
                || crate::driver::irq_router::in_dispatch()
            {
                return Err(a::serial_error_owner);
            }
            let task = scheduler::current();
            if task.is_null() {
                return Err(a::serial_error_owner);
            }
            let c = task_registry::core(task.cast());
            if (*c).held_lock_count != 0 || (*c).preempt_disable_depth != 0 {
                return Err(a::serial_error_owner);
            }
            let native = task_context::Admission::acquire(true).ok_or(a::serial_error_owner)?;
            let h = context::handle().ok_or(a::serial_error_owner)?;
            let mut lease = MaybeUninit::uninit();
            if !registry::r4os_rust_program_registry_pin(h, false, lease.as_mut_ptr()) {
                return Err(a::serial_error_closed);
            }
            Ok(Self {
                lease: lease.assume_init(),
                _native: native,
            })
        }
    }
    fn client(&self) -> serial::Client {
        serial::Client {
            handle: a::ProgramProcessHandle {
                instance_id: self.lease.id,
                generation: self.lease.generation,
                ..Default::default()
            },
            closed: unsafe {
                ptr::addr_of!((*self.lease.instance).serial_closed).cast::<AtomicBool>()
            },
        }
    }
}
impl Drop for Caller {
    fn drop(&mut self) {
        unsafe { registry::unpin(&self.lease) }
    }
}
pub extern "C" fn count() -> u32 {
    serial::count()
}
/// # Safety
/// Caller retains writable aligned versioned output through this call.
pub unsafe extern "C" fn port_at(index: u32, out: *mut a::SerialPortInfo) -> i32 {
    unsafe { serial::port_at(index, out) }
}
/// # Safety
/// Caller retains readable config and exclusive writable handle through return.
pub unsafe extern "C" fn open(
    id: u64,
    generation: u64,
    cfg: *const a::SerialConfig,
    out: *mut u64,
) -> i32 {
    let caller = match Caller::enter() {
        Ok(x) => x,
        Err(e) => return e,
    };
    unsafe { serial::open(caller.client(), id, generation, cfg, out) }
}
/// # Safety
/// Caller retains one readable versioned configuration through return.
pub unsafe extern "C" fn configure(handle: u64, cfg: *const a::SerialConfig) -> i32 {
    let caller = match Caller::enter() {
        Ok(x) => x,
        Err(e) => return e,
    };
    unsafe { serial::configure(caller.client(), handle, cfg) }
}
/// # Safety
/// Caller keeps capacity writable bytes and complete exclusive transfer output
/// alive until return, including every blocking wait and cancellation.
pub unsafe extern "C" fn read(
    handle: u64,
    data: *mut u8,
    capacity: u32,
    ticks: u64,
    out: *mut a::SerialTransfer,
) -> i32 {
    let caller = match Caller::enter() {
        Ok(x) => x,
        Err(e) => return e,
    };
    unsafe { serial::transfer(caller.client(), handle, false, data, capacity, ticks, out) }
}
/// # Safety
/// Caller keeps length readable immutable bytes and exclusive transfer output
/// alive through return. Providers only receive resident bounded copied spans.
pub unsafe extern "C" fn write(
    handle: u64,
    data: *const u8,
    length: u32,
    ticks: u64,
    out: *mut a::SerialTransfer,
) -> i32 {
    let caller = match Caller::enter() {
        Ok(x) => x,
        Err(e) => return e,
    };
    unsafe {
        serial::transfer(
            caller.client(),
            handle,
            true,
            data.cast_mut(),
            length,
            ticks,
            out,
        )
    }
}
pub extern "C" fn drain(handle: u64, ticks: u64) -> i32 {
    let caller = match Caller::enter() {
        Ok(x) => x,
        Err(e) => return e,
    };
    serial::drain(caller.client(), handle, ticks)
}
/// # Safety
/// Caller retains writable aligned versioned status until return.
pub unsafe extern "C" fn status(handle: u64, out: *mut a::SerialStatus) -> i32 {
    let caller = match Caller::enter() {
        Ok(x) => x,
        Err(e) => return e,
    };
    unsafe { serial::status(caller.client(), handle, out) }
}
pub extern "C" fn cancel(handle: u64) -> i32 {
    let caller = match Caller::enter() {
        Ok(x) => x,
        Err(e) => return e,
    };
    serial::cancel(caller.client(), handle)
}
pub extern "C" fn close(handle: u64, flags: u32, ticks: u64) -> i32 {
    let caller = match Caller::enter() {
        Ok(x) => x,
        Err(e) => return e,
    };
    serial::close(caller.client(), handle, flags, ticks)
}
