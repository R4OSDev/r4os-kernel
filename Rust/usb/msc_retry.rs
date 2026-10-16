//! Original bounded transport retry and device-provided SCSI sense policy.
pub const TRANSPORT_RETRY_LIMIT: u8 = 1;
pub fn retry_transport(transport: bool, recovered: bool, retries: u8) -> bool {
    transport && recovered && retries < TRANSPORT_RETRY_LIMIT
}
pub fn transient_not_ready(asc: u8, ascq: u8) -> bool {
    if asc == 0x3a {
        return false;
    }
    if asc != 0x04 {
        return true;
    }
    !matches!(ascq, 0x02 | 0x03 | 0x12)
}
pub fn retry_scsi(
    transport: bool,
    recovered: bool,
    valid: bool,
    sense: u8,
    asc: u8,
    ascq: u8,
    retries: u8,
) -> bool {
    if transport {
        return recovered;
    }
    if !valid {
        return retries == 0;
    }
    match sense {
        0x06 => true,
        0x02 => transient_not_ready(asc, ascq),
        _ => false,
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_usb_retry_transport(
    transport: bool,
    recovered: bool,
    retries: u8,
) -> bool {
    retry_transport(transport, recovered, retries)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_usb_retry_scsi(
    transport: bool,
    recovered: bool,
    valid: bool,
    sense: u8,
    asc: u8,
    ascq: u8,
    retries: u8,
) -> bool {
    retry_scsi(transport, recovered, valid, sense, asc, ascq, retries)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_usb_transient_not_ready(asc: u8, ascq: u8) -> bool {
    transient_not_ready(asc, ascq)
}
