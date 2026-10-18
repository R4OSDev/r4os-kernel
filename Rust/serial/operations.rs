use super::*;
pub(crate) fn count() -> u32 {
    unsafe {
        let _g = RuntimeGuard::acquire();
        let mut p = (*raw()).head;
        let mut n = 0u32;
        while !p.is_null() {
            if (*p).registration != 0 {
                n = n.checked_add(1).expect("serial catalog count");
            }
            p = (*p).next;
        }
        n
    }
}
/// # Safety
/// Caller retains a writable versioned record; no metadata owner spans output.
pub(crate) unsafe fn port_at(index: u32, out: *mut a::SerialPortInfo) -> i32 {
    unsafe {
        if !header(out) {
            return a::serial_error_invalid;
        }
        let info = {
            let _g = RuntimeGuard::acquire();
            let mut p = (*raw()).head;
            let mut n = 0;
            while !p.is_null() && ((*p).registration == 0 || n != index) {
                if (*p).registration != 0 {
                    n += 1
                }
                p = (*p).next;
            }
            if p.is_null() {
                return a::serial_error_not_found;
            }
            let mut value = a::SerialPortInfo {
                port_id: (*p).id,
                generation: (*p).registration,
                com_number: (*p).number,
                flags: a::serial_port_present,
                capabilities: u64::from((*p).backend.capabilities),
                hardware_identity: (*p).identity,
                transport: (*p).backend.resource_kind,
                ..Default::default()
            };
            if reserved(p) {
                value.flags |= a::serial_port_debug_reserved
            }
            if (*p).handle != 0 {
                value.flags |= a::serial_port_open
            }
            if (*p).closing || (*p).provider_closing {
                value.flags |= a::serial_port_closing
            }
            if (*p).last_error == a::serial_error_hardware {
                value.flags |= a::serial_port_hardware_error
            }
            value.name[..3].copy_from_slice(b"COM");
            let mut digits = [0u8; 10];
            let mut number = (*p).number;
            let mut pos = digits.len();
            loop {
                pos -= 1;
                digits[pos] = b'0' + (number % 10) as u8;
                number /= 10;
                if number == 0 {
                    break;
                }
            }
            let len = digits.len() - pos;
            value.name[3..3 + len].copy_from_slice(&digits[pos..]);
            value
        };
        out.write(info);
        a::serial_ok
    }
}
/// # Safety
/// Process generation/closed flag are pinned; immutable config and writable
/// handle live through bounded provider admission and any cancellation cleanup.
pub(crate) unsafe fn open(
    client: Client,
    id: u64,
    generation: u64,
    config: *const a::SerialConfig,
    out: *mut u64,
) -> i32 {
    unsafe {
        if out.is_null() || !(out as usize).is_multiple_of(8) {
            return a::serial_error_invalid;
        }
        out.write(0);
        if !header(config) {
            return a::serial_error_invalid;
        }
        let cfg = config.read();
        if !config_valid(&cfg) {
            return a::serial_error_invalid;
        }
        let (p, d, handle) = {
            let _g = RuntimeGuard::acquire();
            if client.closing() {
                return a::serial_error_closed;
            }
            let p = by_id(id);
            if p.is_null() {
                return a::serial_error_not_found;
            }
            if (*p).registration == 0 || (*p).registration != generation {
                return a::serial_error_stale;
            }
            if reserved(p) || (*p).handle != 0 || (*p).provider_closing {
                return a::serial_error_busy;
            }
            if cfg.flow_control == a::serial_flow_rts_cts as u8
                && (*p).backend.capabilities & a::serial_cap_rts_cts == 0
            {
                return a::serial_error_unsupported;
            }
            let Some(handle) = unique() else {
                return a::serial_error_no_memory;
            };
            (*p).handle = handle;
            (*p).client = client.handle;
            (*p).closing = false;
            (*p).active_io = 1;
            (*p).opening = true;
            (p, (*p).backend, handle)
        };
        let result = invoke_config(p, d, true, &cfg);
        let cancelled = {
            let _g = RuntimeGuard::acquire();
            assert_eq!((*p).handle, handle);
            assert_eq!((*p).active_io, 1);
            (*p).active_io = 0;
            (*p).opening = false;
            (*p).last_error = result;
            if result != 0 {
                (*p).last_closed = handle;
                (*p).handle = 0;
                (*p).closing = false;
                (*p).client = a::ProgramProcessHandle::default();
            }
            let cancelled = client.closing() || (*p).closing || (*p).provider_closing;
            changed(p);
            cancelled
        };
        if result != 0 {
            return result;
        }
        if cancelled {
            let _ = io::close_port(
                p,
                handle,
                generation,
                a::serial_close_abort,
                u64::from(timer::frequency()) * 3,
            );
            return a::serial_error_cancelled;
        }
        out.write(handle);
        a::serial_ok
    }
}
pub(crate) unsafe fn configure(client: Client, handle: u64, input: *const a::SerialConfig) -> i32 {
    unsafe {
        if !header(input) {
            return a::serial_error_invalid;
        }
        let cfg = input.read();
        if !config_valid(&cfg) {
            return a::serial_error_invalid;
        }
        let lease = match Lease::enter(client, handle, false) {
            Ok(x) => x,
            Err(e) => return e,
        };
        if cfg.flow_control == a::serial_flow_rts_cts as u8
            && lease.backend.capabilities & a::serial_cap_rts_cts == 0
        {
            return a::serial_error_unsupported;
        }
        invoke_config(lease.port, lease.backend, false, &cfg)
    }
}
pub(crate) unsafe fn status(client: Client, handle: u64, out: *mut a::SerialStatus) -> i32 {
    unsafe {
        if !header(out) {
            return a::serial_error_invalid;
        }
        let lease = match Lease::enter(client, handle, true) {
            Ok(x) => x,
            Err(e) => return e,
        };
        let mut value = a::SerialStatus::default();
        let code = {
            let _cb = match Callback::enter(lease.port) {
                Ok(x) => x,
                Err(e) => return e,
            };
            let f: unsafe extern "C" fn(u64, *mut a::SerialStatus) -> i32 =
                mem::transmute(lease.backend.status);
            f(lease.backend.context, ptr::addr_of_mut!(value))
        };
        if code != 0 {
            return code;
        }
        if value.version != 1 || value.size < mem::size_of::<a::SerialStatus>() as u32 {
            return a::serial_error_hardware;
        }
        {
            let _g = RuntimeGuard::acquire();
            value.generation = lease.registration;
            value.flags |= a::serial_port_present | a::serial_port_open;
            if (*lease.port).closing {
                value.flags |= a::serial_port_closing
            }
            if (*lease.port).last_error == a::serial_error_hardware {
                value.flags |= a::serial_port_hardware_error;
                value.last_error = a::serial_error_hardware;
            }
        }
        out.write(value);
        a::serial_ok
    }
}
