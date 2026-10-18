//! Serial mediation only: dynamic exact port ownership, module loans and waits.
//! UART registers, rings, IRQ handling and line setup belong to SERIAL.R4D.
mod catalog;
mod io;
mod lifetime;
mod operations;
use crate::{
    arch::runtime::RuntimeGuard,
    contract as a,
    driver::{api_admission, heap as driver_heap},
    memory::heap,
    platform::timer,
    sched::{
        scheduler, sync_objects,
        sync_types::{Text, WaitQueue},
        task::task_registry,
    },
};
pub(crate) use catalog::driver_query;
use core::{
    cell::UnsafeCell,
    ffi::c_void,
    marker::PhantomPinned,
    mem,
    pin::Pin,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};
pub(crate) use io::{cancel, close, drain, transfer};
pub(crate) use lifetime::{cancel_task, driver_close, driver_quiesced, reap_process, stop_process};
pub(crate) use operations::{configure, count, open, port_at, status};
#[derive(Clone, Copy)]
pub(crate) struct Client {
    pub handle: a::ProgramProcessHandle,
    pub closed: *const AtomicBool,
}
fn same(a: a::ProgramProcessHandle, b: a::ProgramProcessHandle) -> bool {
    a.instance_id == b.instance_id && a.generation == b.generation
}
impl Client {
    fn closing(self) -> bool {
        self.closed.is_null() || unsafe { (*self.closed).load(Ordering::Acquire) }
    }
}
struct Port {
    next: *mut Port,
    id: u64,
    registration: u64,
    provider: u32,
    epoch: u64,
    number: u32,
    identity: [u8; 64],
    backend: a::SerialBackend,
    handle: u64,
    last_closed: u64,
    client: a::ProgramProcessHandle,
    active_io: u32,
    callbacks: u32,
    closing: bool,
    opening: bool,
    provider_closing: bool,
    close_running: bool,
    last_error: i32,
    sequence: u64,
    queue: WaitQueue,
    waiters: *mut Waiter,
}
impl Port {
    fn empty() -> Self {
        Self {
            next: ptr::null_mut(),
            id: 0,
            registration: 0,
            provider: 0,
            epoch: 0,
            number: 0,
            identity: [0; 64],
            backend: a::SerialBackend::default(),
            handle: 0,
            last_closed: 0,
            client: a::ProgramProcessHandle::default(),
            active_io: 0,
            callbacks: 0,
            closing: false,
            opening: false,
            provider_closing: false,
            close_running: false,
            last_error: 0,
            sequence: 0,
            queue: WaitQueue::EMPTY,
            waiters: ptr::null_mut(),
        }
    }
}
struct Catalog {
    head: *mut Port,
    next: u64,
}
struct Storage(UnsafeCell<Catalog>);
// SAFETY: RuntimeGuard protects every metadata access. Nodes retain only
// naming metadata after unregister and stay resident until this boot ends.
unsafe impl Sync for Storage {}
static CATALOG: Storage = Storage(UnsafeCell::new(Catalog {
    head: ptr::null_mut(),
    next: 1,
}));
fn raw() -> *mut Catalog {
    CATALOG.0.get()
}
unsafe fn unique() -> Option<u64> {
    unsafe {
        let s = raw();
        let n = (*s).next;
        (*s).next = n.checked_add(1)?;
        Some(n)
    }
}
unsafe fn by_registration(id: u64) -> *mut Port {
    unsafe {
        let mut p = (*raw()).head;
        while !p.is_null() {
            if id != 0 && (*p).registration == id {
                return p;
            }
            p = (*p).next;
        }
        ptr::null_mut()
    }
}
unsafe fn by_id(id: u64) -> *mut Port {
    unsafe {
        let mut p = (*raw()).head;
        while !p.is_null() {
            if (*p).id == id {
                return p;
            }
            p = (*p).next;
        }
        ptr::null_mut()
    }
}
unsafe fn by_handle(id: u64) -> Result<*mut Port, i32> {
    unsafe {
        let mut p = (*raw()).head;
        while !p.is_null() {
            if id != 0 && (*p).handle == id {
                return Ok(p);
            }
            if id != 0 && (*p).last_closed == id {
                return Err(a::serial_error_closed);
            }
            p = (*p).next;
        }
        Err(a::serial_error_stale)
    }
}
unsafe fn reserved(p: *const Port) -> bool {
    unsafe {
        crate::boot::serial::debug_claims_com1()
            && ((*p).number == 1
                || ((*p).backend.resource_kind == a::serial_resource_io
                    && (*p).backend.resource_base < 0x400
                    && (*p)
                        .backend
                        .resource_base
                        .saturating_add((*p).backend.resource_length)
                        > 0x3f8))
    }
}
unsafe fn changed(p: *mut Port) {
    unsafe {
        (*p).sequence = (*p).sequence.wrapping_add(1);
        sync_objects::queue_wake_all(ptr::addr_of_mut!((*p).queue));
    }
}
fn provider(closing: bool) -> Option<(u32, u64)> {
    let id = api_admission::r4os_rust_driver_api_runtime_owner(false);
    if id == 0 {
        return None;
    }
    let mut s = a::DriverHeapStats::default();
    if unsafe { driver_heap::r4os_rust_driver_heap_stats(id, &mut s) } != 0
        || s.owner_epoch == 0
        || (!closing && s.closing != 0)
    {
        None
    } else {
        Some((id, s.owner_epoch))
    }
}
pub(crate) unsafe fn header<T>(out: *const T) -> bool {
    if out.is_null() || !(out as usize).is_multiple_of(mem::align_of::<T>()) {
        return false;
    }
    unsafe {
        let p = out.cast::<u32>();
        p.read() == 1 && p.add(1).read() >= mem::size_of::<T>() as u32
    }
}
pub(crate) fn config_valid(c: &a::SerialConfig) -> bool {
    c.version == 1
        && c.size >= 32
        && c.baud_rate != 0
        && (5..=8).contains(&c.data_bits)
        && c.parity <= 4
        && matches!(c.stop_bits, 1 | 2)
        && c.flow_control <= 1
        && c.flags == 0
        && c.reserved0 == 0
        && c.reserved1 == 0
        && c.reserved2 == 0
}
struct Callback {
    port: *mut Port,
    token: api_admission::CallbackToken,
}
impl Callback {
    unsafe fn enter(p: *mut Port) -> Result<Self, i32> {
        unsafe {
            let owner = {
                let _g = RuntimeGuard::acquire();
                if (*p).registration == 0 || (*p).callbacks == u32::MAX {
                    return Err(a::serial_error_busy);
                }
                (*p).callbacks += 1;
                (*p).provider
            };
            let mut token = mem::MaybeUninit::uninit();
            if !api_admission::r4os_rust_driver_api_storage_enter(owner, token.as_mut_ptr()) {
                let _g = RuntimeGuard::acquire();
                (*p).callbacks -= 1;
                return Err(a::serial_error_busy);
            }
            Ok(Self {
                port: p,
                token: token.assume_init(),
            })
        }
    }
}
impl Drop for Callback {
    fn drop(&mut self) {
        unsafe {
            api_admission::r4os_rust_driver_api_storage_leave(&self.token);
            let _g = RuntimeGuard::acquire();
            assert!((*self.port).callbacks != 0);
            (*self.port).callbacks -= 1;
        }
    }
}
struct Lease {
    port: *mut Port,
    registration: u64,
    handle: u64,
    backend: a::SerialBackend,
}
impl Lease {
    fn enter(client: Client, handle: u64, allow_closing: bool) -> Result<Self, i32> {
        unsafe {
            let _g = RuntimeGuard::acquire();
            let p = by_handle(handle)?;
            if !same((*p).client, client.handle) {
                return Err(a::serial_error_owner);
            }
            if client.closing() || (*p).provider_closing || (*p).closing && !allow_closing {
                return Err(a::serial_error_closed);
            }
            if (*p).opening || (*p).close_running || (*p).active_io == u32::MAX {
                return Err(a::serial_error_busy);
            }
            (*p).active_io += 1;
            Ok(Self {
                port: p,
                registration: (*p).registration,
                handle,
                backend: (*p).backend,
            })
        }
    }
    fn valid(&self) -> bool {
        unsafe {
            (*self.port).registration == self.registration && (*self.port).handle == self.handle
        }
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        let _g = RuntimeGuard::acquire();
        unsafe {
            assert!(self.valid() && (*self.port).active_io != 0);
            (*self.port).active_io -= 1;
            if (*self.port).closing || (*self.port).provider_closing {
                changed(self.port);
            }
        }
    }
}
struct Waiter {
    next: *mut Waiter,
    port: *mut Port,
    task: u32,
    generation: u64,
    handle: u64,
    registration: u64,
    observed: u64,
    cancelled: bool,
    linked: bool,
    _pin: PhantomPinned,
}
impl Waiter {
    fn new(lease: &Lease) -> Self {
        Self {
            next: ptr::null_mut(),
            port: lease.port,
            task: 0,
            generation: 0,
            handle: lease.handle,
            registration: lease.registration,
            observed: 0,
            cancelled: false,
            linked: false,
            _pin: PhantomPinned,
        }
    }
    fn link(self: Pin<&mut Self>) {
        unsafe {
            let this = self.get_unchecked_mut();
            let _g = RuntimeGuard::acquire();
            let task = scheduler::current();
            assert!(!task.is_null());
            let c = task_registry::core(task.cast());
            this.task = (*c).id;
            this.generation = (*c).generation;
            this.next = (*this.port).waiters;
            (*this.port).waiters = this;
            this.linked = true;
        }
    }
    fn snapshot(self: Pin<&mut Self>) -> bool {
        unsafe {
            let this = self.get_unchecked_mut();
            let _g = RuntimeGuard::acquire();
            this.observed = (*this.port).sequence;
            !this.cancelled
                && (*this.port).registration == this.registration
                && (*this.port).handle == this.handle
                && !(*this.port).closing
                && !(*this.port).provider_closing
        }
    }
    fn cancelled(self: Pin<&Self>) -> bool {
        let _g = RuntimeGuard::acquire();
        self.cancelled
    }
    fn wait(self: Pin<&mut Self>, ticks: u64) {
        unsafe {
            let this = self.get_unchecked_mut();
            let queue = ptr::addr_of_mut!((*this.port).queue);
            let _ = sync_objects::queue_wait_unless(
                queue,
                ticks,
                Text::literal(b"serial-io"),
                still_waiting,
                ptr::from_mut(this).cast(),
            );
        }
    }
}
impl Drop for Waiter {
    fn drop(&mut self) {
        if !self.linked {
            return;
        }
        unsafe {
            let _g = RuntimeGuard::acquire();
            let mut link = ptr::addr_of_mut!((*self.port).waiters);
            while !(*link).is_null() {
                if ptr::eq(*link, self) {
                    *link = self.next;
                    self.linked = false;
                    return;
                }
                link = ptr::addr_of_mut!((**link).next);
            }
            panic!("serial wait identity");
        }
    }
}
unsafe extern "C" fn still_waiting(raw: *mut c_void) -> bool {
    unsafe {
        let w = &*raw.cast::<Waiter>();
        !w.cancelled
            && (*w.port).registration == w.registration
            && (*w.port).handle == w.handle
            && !(*w.port).closing
            && !(*w.port).provider_closing
            && (*w.port).sequence == w.observed
    }
}
struct Budget {
    start: u64,
    ticks: u64,
}
impl Budget {
    fn new(ticks: u64) -> Result<Self, i32> {
        if ticks > i64::MAX as u64 {
            Err(a::serial_error_too_large)
        } else {
            Ok(Self {
                start: timer::tick_count(),
                ticks,
            })
        }
    }
    fn remaining(&self) -> u64 {
        self.ticks
            .saturating_sub(timer::tick_count().wrapping_sub(self.start))
    }
    fn exhausted(&self) -> i32 {
        if self.ticks == 0 {
            a::serial_error_would_block
        } else {
            a::serial_error_timeout
        }
    }
}
unsafe fn invoke_config(
    p: *mut Port,
    d: a::SerialBackend,
    opening: bool,
    c: &a::SerialConfig,
) -> i32 {
    unsafe {
        let _cb = match Callback::enter(p) {
            Ok(x) => x,
            Err(e) => return e,
        };
        let f: unsafe extern "C" fn(u64, *const a::SerialConfig) -> i32 =
            mem::transmute(if opening { d.open } else { d.configure });
        f(d.context, c)
    }
}
unsafe fn invoke_drain(p: *mut Port, d: a::SerialBackend) -> i32 {
    unsafe {
        let _cb = match Callback::enter(p) {
            Ok(x) => x,
            Err(e) => return e,
        };
        let f: unsafe extern "C" fn(u64) -> i32 = mem::transmute(d.drain);
        f(d.context)
    }
}
unsafe fn invoke_close(p: *mut Port, d: a::SerialBackend, flags: u32) -> i32 {
    unsafe {
        let _cb = match Callback::enter(p) {
            Ok(x) => x,
            Err(e) => return e,
        };
        let f: unsafe extern "C" fn(u64, u32) -> i32 = mem::transmute(d.close);
        f(d.context, flags)
    }
}
