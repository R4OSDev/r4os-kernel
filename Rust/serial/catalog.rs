use super::*;
fn descriptor_valid(d: &a::SerialBackend) -> bool {
    d.com_number != 0
        && d.resource_length != 0
        && d.resource_base.checked_add(d.resource_length).is_some()
        && matches!(d.resource_kind, 1..=3)
        && (d.resource_kind == a::serial_resource_opaque && d.irq == 255
            || d.irq < 32 && ![0, 1, 12].contains(&d.irq))
        && d.hardware_identity[0] != 0
        && d.hardware_identity.contains(&0)
        && d.hardware_identity
            .iter()
            .skip_while(|&&x| x != 0)
            .all(|&x| x == 0)
        && d.hardware_identity
            .iter()
            .take_while(|&&x| x != 0)
            .all(|x| x.is_ascii_alphanumeric() || b"_.-".contains(x))
        && [
            d.open,
            d.configure,
            d.read,
            d.write,
            d.drain,
            d.status,
            d.close,
        ]
        .iter()
        .all(|&x| x != 0)
        && (d.resource_kind != a::serial_resource_io
            || d.resource_base + d.resource_length <= 65536)
}
/// # Safety
/// Exclusive versioned resident output; caller retains authentic R4D context.
pub(crate) unsafe extern "C" fn driver_query(out: *mut a::DriverSerialApi) -> i32 {
    unsafe {
        if !header(out) {
            return a::serial_error_invalid;
        }
        if crate::driver::irq_router::in_dispatch() {
            return a::serial_error_owner;
        }
        if provider(false).is_none() || api_admission::r4os_rust_driver_api_owner_scalar(1) == 0 {
            return a::serial_error_owner;
        }
        out.write(a::DriverSerialApi {
            register_port: register as *const () as u64,
            unregister_port: unregister as *const () as u64,
            notify: notify as *const () as u64,
            admission: admission as *const () as u64,
            ..Default::default()
        });
        a::serial_ok
    }
}
unsafe extern "C" fn register(input: *const a::SerialBackend, out: *mut u64) -> i32 {
    unsafe {
        if out.is_null() || !(out as usize).is_multiple_of(8) {
            return a::serial_error_invalid;
        }
        out.write(0);
        if !header(input)
            || crate::driver::irq_router::in_dispatch()
            || api_admission::r4os_rust_driver_api_owner_scalar(1) == 0
        {
            return a::serial_error_invalid;
        }
        let d = input.read();
        if !descriptor_valid(&d) {
            return a::serial_error_invalid;
        }
        let Some((owner, epoch)) = provider(false) else {
            return a::serial_error_owner;
        };
        // Allocate outside metadata. A concurrent duplicate frees only this unpublished
        // candidate; no allocation/read/write of module data occurs under RuntimeGuard.
        let candidate = heap::r4os_rust_heap_alloc(mem::size_of::<Port>(), mem::align_of::<Port>())
            .cast::<Port>();
        if candidate.is_null() {
            return a::serial_error_no_memory;
        }
        candidate.write(Port::empty());
        let result = {
            let _g = RuntimeGuard::acquire();
            let mut p = (*raw()).head;
            let mut found = ptr::null_mut();
            let mut result = a::serial_ok;
            let mut registration = 0;
            while !p.is_null() {
                if (*p).identity == d.hardware_identity {
                    if (*p).number != d.com_number || (*p).registration != 0 {
                        result = a::serial_error_busy;
                        break;
                    }
                    found = p;
                } else if (*p).number == d.com_number {
                    result = a::serial_error_busy;
                    break;
                }
                if (*p).registration != 0
                    && (*p).backend.resource_kind == d.resource_kind
                    && d.resource_kind != a::serial_resource_opaque
                    && d.resource_base < (*p).backend.resource_base + (*p).backend.resource_length
                    && (*p).backend.resource_base < d.resource_base + d.resource_length
                {
                    result = a::serial_error_busy;
                    break;
                }
                p = (*p).next;
            }
            if result == 0 {
                let new = found.is_null();
                if new {
                    found = candidate;
                    match unique() {
                        Some(x) => (*found).id = x,
                        None => result = a::serial_error_no_memory,
                    }
                }
                if result == 0 {
                    if let Some(id) = unique() {
                        (*found).registration = id;
                        (*found).provider = owner;
                        (*found).epoch = epoch;
                        (*found).number = d.com_number;
                        (*found).identity = d.hardware_identity;
                        (*found).backend = d;
                        (*found).provider_closing = false;
                        (*found).closing = false;
                        (*found).last_error = 0;
                        if new {
                            (*found).next = (*raw()).head;
                            (*raw()).head = found;
                        }
                        registration = id;
                        changed(found);
                    } else {
                        result = a::serial_error_no_memory
                    }
                }
            }
            (result, found == candidate && result == 0, registration)
        };
        if !result.1 {
            assert_eq!(
                heap::r4os_rust_heap_free(candidate.cast(), mem::size_of::<Port>()),
                0
            )
        }
        if result.0 == 0 {
            out.write(result.2)
        }
        result.0
    }
}
unsafe extern "C" fn unregister(id: u64) -> i32 {
    unsafe {
        let Some((owner, epoch)) = provider(true) else {
            return a::serial_error_owner;
        };
        let _g = RuntimeGuard::acquire();
        let p = by_registration(id);
        if p.is_null() {
            return a::serial_error_stale;
        }
        if (*p).provider != owner || (*p).epoch != epoch {
            return a::serial_error_owner;
        }
        if (*p).handle != 0
            || (*p).active_io != 0
            || (*p).callbacks != 0
            || (*p).close_running
            || !(*p).waiters.is_null()
        {
            return a::serial_error_busy;
        }
        (*p).registration = 0;
        (*p).provider = 0;
        (*p).epoch = 0;
        (*p).backend = a::SerialBackend::default();
        changed(p);
        a::serial_ok
    }
}
unsafe extern "C" fn admission(id: u64) -> i32 {
    unsafe {
        let Some((owner, epoch)) = provider(true) else {
            return a::serial_error_owner;
        };
        let _g = RuntimeGuard::acquire();
        let p = by_registration(id);
        if p.is_null() {
            return a::serial_error_stale;
        }
        if (*p).provider != owner || (*p).epoch != epoch {
            return a::serial_error_owner;
        }
        if reserved(p) || (*p).provider_closing {
            a::serial_error_busy
        } else {
            a::serial_ok
        }
    }
}
/// # Safety
/// Exact live provider registration in authenticated task or IRQ context.
pub(crate) unsafe extern "C" fn notify(id: u64, events: u32) -> i32 {
    unsafe {
        if events == 0 || events & !31 != 0 {
            return a::serial_error_invalid;
        }
        let Some((owner, epoch)) = provider(true) else {
            return a::serial_error_owner;
        };
        let _g = RuntimeGuard::acquire();
        let p = by_registration(id);
        if p.is_null() {
            return a::serial_error_stale;
        }
        if (*p).provider != owner || (*p).epoch != epoch {
            return a::serial_error_owner;
        }
        changed(p);
        a::serial_ok
    }
}
