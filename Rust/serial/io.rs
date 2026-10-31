use super::*;
fn finish(out: *mut a::SerialTransfer, mut value: a::SerialTransfer, code: i32) -> i32 {
    value.status = code;
    if code != 0 && value.completed != 0 {
        value.flags |= a::serial_transfer_partial
    }
    unsafe { out.write(value) }
    code
}
/// # Safety
/// Exact caller process and all byte/output storage stay pinned until return.
/// Foreign providers only see resident bounce storage during bounded callbacks.
pub(crate) unsafe fn transfer(
    client: Client,
    handle: u64,
    write: bool,
    data: *mut u8,
    len: u32,
    ticks: u64,
    out: *mut a::SerialTransfer,
) -> i32 {
    unsafe {
        if !header(out) {
            return a::serial_error_invalid;
        }
        let mut value = a::SerialTransfer {
            requested: len,
            ..Default::default()
        };
        if len > a::serial_max_transfer_bytes {
            return finish(out, value, a::serial_error_too_large);
        }
        if len != 0 && data.is_null() {
            return finish(out, value, a::serial_error_invalid);
        }
        let budget = match Budget::new(ticks) {
            Ok(x) => x,
            Err(e) => return finish(out, value, e),
        };
        let lease = match Lease::enter(client, handle, false) {
            Ok(x) => x,
            Err(e) => return finish(out, value, e),
        };
        if len == 0 {
            return finish(out, value, a::serial_ok);
        }
        let mut waiter = core::pin::pin!(Waiter::new(&lease));
        waiter.as_mut().link();
        let mut bounce = [0u8; a::serial_bounce_bytes as usize];
        loop {
            if !waiter.as_mut().snapshot() || client.closing() {
                return finish(out, value, a::serial_error_cancelled);
            }
            let size = (len - value.completed).min(a::serial_bounce_bytes);
            if write {
                ptr::copy_nonoverlapping(
                    data.add(value.completed as usize),
                    bounce.as_mut_ptr(),
                    size as usize,
                );
            }
            let mut result = a::SerialTransfer {
                requested: size,
                ..Default::default()
            };
            let code = {
                let _cb = match Callback::enter(lease.port) {
                    Ok(x) => x,
                    Err(e) => return finish(out, value, e),
                };
                if write {
                    let f: unsafe extern "C" fn(
                        u64,
                        *const u8,
                        u32,
                        *mut a::SerialTransfer,
                    ) -> i32 = mem::transmute(lease.backend.write);
                    f(
                        lease.backend.context,
                        bounce.as_ptr(),
                        size,
                        ptr::addr_of_mut!(result),
                    )
                } else {
                    let f: unsafe extern "C" fn(u64, *mut u8, u32, *mut a::SerialTransfer) -> i32 =
                        mem::transmute(lease.backend.read);
                    f(
                        lease.backend.context,
                        bounce.as_mut_ptr(),
                        size,
                        ptr::addr_of_mut!(result),
                    )
                }
            };
            if result.completed > size || result.version != 1 || result.size < 32 {
                return finish(out, value, a::serial_error_hardware);
            }
            if !write && result.completed != 0 {
                ptr::copy_nonoverlapping(
                    bounce.as_ptr(),
                    data.add(value.completed as usize),
                    result.completed as usize,
                )
            }
            value.completed += result.completed;
            value.flags |= result.flags;
            value.overflows = value.overflows.saturating_add(result.overflows);
            if code != a::serial_ok && code != a::serial_error_would_block {
                return finish(out, value, code);
            }
            if value.completed == len || !write && value.completed != 0 {
                return finish(out, value, a::serial_ok);
            }
            if waiter.as_ref().cancelled() || client.closing() {
                return finish(out, value, a::serial_error_cancelled);
            }
            let left = budget.remaining();
            if left == 0 {
                return finish(out, value, budget.exhausted());
            }
            // A partial successful write retries from its exact accepted prefix. With
            // no progress it sleeps on a real sequence change, never a yield loop.
            if result.completed == 0 {
                waiter.as_mut().wait(left);
            }
        }
    }
}
pub(crate) fn drain(client: Client, handle: u64, ticks: u64) -> i32 {
    unsafe {
        let budget = match Budget::new(ticks) {
            Ok(x) => x,
            Err(e) => return e,
        };
        let lease = match Lease::enter(client, handle, false) {
            Ok(x) => x,
            Err(e) => return e,
        };
        let mut waiter = core::pin::pin!(Waiter::new(&lease));
        waiter.as_mut().link();
        loop {
            if !waiter.as_mut().snapshot() || client.closing() {
                return a::serial_error_cancelled;
            }
            let code = invoke_drain(lease.port, lease.backend);
            if code != a::serial_error_would_block {
                return code;
            }
            let left = budget.remaining();
            if left == 0 {
                return budget.exhausted();
            } // UART has no TEMT interrupt: one bounded timer recheck after THRE.
            waiter.as_mut().wait(left.min(1));
        }
    }
}
pub(crate) fn cancel(client: Client, handle: u64) -> i32 {
    unsafe {
        let _g = RuntimeGuard::acquire();
        let p = match by_handle(handle) {
            Ok(x) => x,
            Err(e) => return e,
        };
        if !same((*p).client, client.handle) {
            return a::serial_error_owner;
        }
        cancel_waiters(p);
        changed(p);
        a::serial_ok
    }
}
unsafe fn cancel_waiters(p: *mut Port) {
    unsafe {
        let mut w = (*p).waiters;
        while !w.is_null() {
            (*w).cancelled = true;
            w = (*w).next;
        }
    }
}
struct Closing(*mut Port);
impl Drop for Closing {
    fn drop(&mut self) {
        unsafe {
            let _g = RuntimeGuard::acquire();
            assert!((*self.0).close_running);
            (*self.0).close_running = false;
            notify_retire(self.0);
            changed(self.0);
        }
    }
}
unsafe extern "C" fn close_waiting(raw: *mut c_void) -> bool {
    unsafe {
        let p = raw.cast::<Port>();
        (*p).active_io != 0
    }
}
pub(super) unsafe fn close_port(
    p: *mut Port,
    expected_handle: u64,
    registration: u64,
    flags: u32,
    ticks: u64,
) -> i32 {
    unsafe {
        if flags & !a::serial_close_abort != 0 {
            return a::serial_error_invalid;
        }
        let budget = match Budget::new(ticks) {
            Ok(x) => x,
            Err(e) => return e,
        };
        let (d, handle) = {
            let _g = RuntimeGuard::acquire();
            // Lookup and admission are separate runtime edges. A different open
            // may already own this resident node; never close its new handle.
            if (*p).registration != registration || (*p).handle != expected_handle {
                return if (*p).last_closed == expected_handle {
                    a::serial_error_closed
                } else {
                    a::serial_error_stale
                };
            }
            if (*p).opening || (*p).close_running {
                return a::serial_error_busy;
            }
            (*p).close_running = true;
            (*p).closing = true;
            cancel_waiters(p);
            changed(p);
            ((*p).backend, (*p).handle)
        };
        let _closing = Closing(p);
        loop {
            let active = {
                let _g = RuntimeGuard::acquire();
                (*p).active_io
            };
            if active != 0 {
                let left = budget.remaining();
                if left == 0 {
                    return budget.exhausted();
                }
                sync_objects::queue_wait_unless(
                    ptr::addr_of_mut!((*p).queue),
                    left,
                    Text::literal(b"serial-close"),
                    close_waiting,
                    p.cast(),
                );
                continue;
            }
            let code = invoke_close(p, d, flags);
            if code == a::serial_ok {
                let _g = RuntimeGuard::acquire();
                assert!((*p).handle == handle && (*p).active_io == 0);
                (*p).last_closed = handle;
                (*p).handle = 0;
                (*p).client = a::ProgramProcessHandle::default();
                (*p).closing = false;
                (*p).last_error = 0;
                return a::serial_ok;
            }
            if code != a::serial_error_would_block {
                let _g = RuntimeGuard::acquire();
                (*p).last_error = code;
                return code;
            }
            let left = budget.remaining();
            if left == 0 {
                return budget.exhausted();
            }
            let reason = b"serial-temt";
            scheduler::sleep(left.min(1), reason.as_ptr(), reason.len());
        }
    }
}
pub(crate) fn close(client: Client, handle: u64, flags: u32, ticks: u64) -> i32 {
    unsafe {
        let (p, registration) = {
            let _g = RuntimeGuard::acquire();
            let p = match by_handle(handle) {
                Ok(x) => x,
                Err(e) => return e,
            };
            if !same((*p).client, client.handle) {
                return a::serial_error_owner;
            }
            (p, (*p).registration)
        };
        close_port(p, handle, registration, flags, ticks)
    }
}
