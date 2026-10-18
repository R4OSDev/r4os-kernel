use super::*;
/// Called with the exact target Task generation before kill/retire deferral.
/// Only serial-owned wait nodes are cancelled; no work completion is signaled.
pub(crate) fn cancel_task(task: u32, generation: u64) {
    unsafe {
        let _g = RuntimeGuard::acquire();
        let mut p = (*raw()).head;
        while !p.is_null() {
            let mut w = (*p).waiters;
            let mut woke = false;
            while !w.is_null() {
                if (*w).task == task && (*w).generation == generation {
                    (*w).cancelled = true;
                    woke = true
                }
                w = (*w).next;
            }
            if woke {
                changed(p)
            }
            p = (*p).next;
        }
    }
}
pub(crate) fn stop_process(client: a::ProgramProcessHandle) {
    unsafe {
        let _g = RuntimeGuard::acquire();
        let mut p = (*raw()).head;
        while !p.is_null() {
            if (*p).handle != 0 && same((*p).client, client) {
                (*p).closing = true;
                let mut w = (*p).waiters;
                while !w.is_null() {
                    (*w).cancelled = true;
                    w = (*w).next;
                }
                changed(p);
            }
            p = (*p).next;
        }
    }
}
pub(crate) fn reap_process(client: a::ProgramProcessHandle) -> bool {
    unsafe {
        stop_process(client);
        let mut p = {
            let _g = RuntimeGuard::acquire();
            (*raw()).head
        };
        while !p.is_null() {
            let (owned, next, handle, registration) = {
                let _g = RuntimeGuard::acquire();
                (
                    (*p).handle != 0 && same((*p).client, client),
                    (*p).next,
                    (*p).handle,
                    (*p).registration,
                )
            };
            if owned
                && io::close_port(
                    p,
                    handle,
                    registration,
                    a::serial_close_abort,
                    u64::from(timer::frequency()) * 3,
                ) != a::serial_ok
            {
                return false;
            }
            p = next;
        }
        true
    }
}
/// Closes admission before DriverShutdown. Waiting happens only on this exact
/// owner's callbacks, never under catalog RuntimeGuard or a registry lock.
pub(crate) fn driver_close(owner: u32) -> bool {
    unsafe {
        let mut p = {
            let _g = RuntimeGuard::acquire();
            let mut p = (*raw()).head;
            while !p.is_null() {
                if (*p).registration != 0 && (*p).provider == owner {
                    (*p).provider_closing = true;
                    (*p).closing = true;
                    let mut w = (*p).waiters;
                    while !w.is_null() {
                        (*w).cancelled = true;
                        w = (*w).next;
                    }
                    changed(p)
                }
                p = (*p).next;
            }
            (*raw()).head
        };
        while !p.is_null() {
            let (owned, next, handle, registration) = {
                let _g = RuntimeGuard::acquire();
                (
                    (*p).registration != 0 && (*p).provider == owner,
                    (*p).next,
                    (*p).handle,
                    (*p).registration,
                )
            };
            if owned
                && handle != 0
                && io::close_port(
                    p,
                    handle,
                    registration,
                    a::serial_close_abort,
                    u64::from(timer::frequency()) * 3,
                ) != a::serial_ok
            {
                return false;
            }
            p = next;
        }
        true
    }
}
/// Unregister must consume every live descriptor before context/IRQ/heap unload.
pub(crate) fn driver_quiesced(owner: u32) -> bool {
    unsafe {
        let _g = RuntimeGuard::acquire();
        let mut p = (*raw()).head;
        while !p.is_null() {
            if (*p).registration != 0 && (*p).provider == owner {
                return false;
            }
            p = (*p).next;
        }
        true
    }
}
